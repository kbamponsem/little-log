//! The widget's headless path: drain queued taps, refresh the widget state.
//!
//! Kotlin owns the widget surface; this module owns the data. Both the app
//! (`AppState`) and the Android `WidgetBridge` (called from a WorkManager
//! worker, with no UI and no Tauri runtime) go through here, so there is
//! exactly one definition of "where the widget reads and writes".

use std::path::{Path, PathBuf};

#[cfg(any(target_os = "android", test))]
use little_log_core::model::{EntryKind, FeedKind};
use little_log_core::pending::{self, DrainReport, WidgetState};
#[cfg(any(target_os = "android", test))]
use little_log_core::timer::{active_timers, TimerKind};
use little_log_core::validate::now_millis;
use little_log_core::{Repo, Result};

/// The four files that make up the widget's world, all inside the app's data
/// directory (the same directory the app's SQLite file lives in).
pub struct WidgetPaths {
    pub data_dir: PathBuf,
}

/// How many minutes the device is ahead of UTC — the widget's notion of "today".
pub fn tz_offset_minutes() -> i32 {
    little_log_core::validate::tz_offset_minutes()
}

impl WidgetPaths {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn database(&self) -> PathBuf {
        self.data_dir.join("little-log.sqlite3")
    }

    pub fn photos(&self) -> PathBuf {
        self.data_dir.join("photos")
    }

    /// Where the Kotlin receiver appends one-tap actions.
    pub fn queue(&self) -> PathBuf {
        self.data_dir.join("pending_actions.json")
    }

    /// The action a widget tap wants the UI to open, handed over by Kotlin.
    pub fn pending_action(&self) -> PathBuf {
        self.data_dir.join("pending_action.txt")
    }

    /// What Glance renders: last feed, sleeping-since, today's counts.
    pub fn state(&self) -> PathBuf {
        self.data_dir.join("widget_state.json")
    }

    /// Written by the app so the worker opens the *same* database even if
    /// Tauri's data directory ever differs from Kotlin's `filesDir`.
    pub fn root_marker(&self) -> PathBuf {
        self.data_dir.join("widget_root.txt")
    }

    pub fn open_repo(&self) -> Result<Repo> {
        Repo::open(self.database())
    }

    /// Remember where the app keeps its data (called once on startup).
    pub fn publish_root(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir).map_err(|error| {
            little_log_core::CoreError::io(self.data_dir.display().to_string(), error)
        })?;
        std::fs::write(self.root_marker(), self.data_dir.display().to_string()).map_err(|error| {
            little_log_core::CoreError::io(self.root_marker().display().to_string(), error)
        })
    }

    /// Follow the app's marker, falling back to our own directory.
    pub fn discover(files_dir: impl AsRef<Path>) -> Self {
        let files_dir = files_dir.as_ref();
        let root = std::fs::read_to_string(files_dir.join("widget_root.txt"))
            .map(|text| text.trim().to_string())
            .ok()
            .filter(|text| !text.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| files_dir.to_path_buf());
        Self::new(root)
    }
}

/// Take the action a widget tap asked the app to open, if one is waiting.
///
/// Read-and-delete, so the sheet opens once rather than on every resume. The tap
/// itself logs nothing: the UI opens that section's sheet and the entry is
/// written when the user saves it.
pub fn take_pending_action(paths: &WidgetPaths) -> Result<Option<String>> {
    let path = paths.pending_action();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(little_log_core::CoreError::io(
                path.display().to_string(),
                error,
            ))
        }
    };
    // Gone whether or not it turns out to be usable, so a bad value cannot wedge
    // the app into reopening the same sheet forever.
    let _ = std::fs::remove_file(&path);

    let action = text.trim();
    if action.is_empty() {
        return Ok(None);
    }
    Ok(Some(action.to_string()))
}

/// Drain queued one-tap actions and rewrite the widget state.
pub fn drain_and_refresh(
    paths: &WidgetPaths,
    tz_offset_minutes: i32,
) -> Result<(DrainReport, WidgetState)> {
    let repo = paths.open_repo()?;
    let report = pending::drain_into(&paths.queue(), &repo, None)?;
    let state = refresh_from(&repo, paths, tz_offset_minutes)?;
    Ok((report, state))
}

/// Rewrite the widget state file from the database (no draining).
// Called from the Android bridge only, so it is "dead" on desktop builds.
#[allow(dead_code)]
pub fn refresh(paths: &WidgetPaths, tz_offset_minutes: i32) -> Result<WidgetState> {
    let repo = paths.open_repo()?;
    refresh_from(&repo, paths, tz_offset_minutes)
}

fn refresh_from(repo: &Repo, paths: &WidgetPaths, tz_offset_minutes: i32) -> Result<WidgetState> {
    let state = pending::widget_state(repo, now_millis(), tz_offset_minutes)?;
    pending::write_widget_state(&paths.state(), &state)?;
    Ok(state)
}

/* ----------------------------------------------------------- session timers */

/// Every running timer as JSON, for the Android notification renderer.
///
/// This is the headless path's source of truth: the notification is rebuilt
/// from the open entries, so a process restart needs no remembered state.
#[cfg(any(target_os = "android", test))]
pub fn active_timers_json(paths: &WidgetPaths) -> Result<String> {
    let repo = paths.open_repo()?;
    let timers = active_timers(&repo)?;
    serde_json::to_string(&timers)
        .map_err(|error| little_log_core::CoreError::data(error.to_string()))
}

/// Stop one open entry (feed or sleep) by id, as an edit event.
///
/// The notification action carries the id, so a stale notification can never
/// stop a newer timer for the same child.
#[cfg(any(target_os = "android", test))]
pub fn stop_timer_entry(paths: &WidgetPaths, entry_id: &str) -> Result<bool> {
    let repo = paths.open_repo()?;
    let Some(mut entry) = repo.entry(entry_id)? else {
        return Ok(false);
    };
    if entry.deleted || entry.ended_at.is_some() || TimerKind::of_entry(entry.kind).is_none() {
        return Ok(false);
    }
    let key = repo.device_key()?;
    let mut clock = repo.clock(key.id())?;
    let now = now_millis();
    // The same clamp the widget uses: a same-millisecond stop still has to be
    // a valid entry, and a sleep is represented in whole minutes.
    let minimum = if entry.kind == EntryKind::Sleep {
        60_000
    } else {
        1
    };
    entry.ended_at = Some(now.max(entry.occurred_at + minimum));
    entry.updated_at = now;
    repo.record_new_entry(&key, &mut clock, &entry)?;
    Ok(true)
}

/// Flip a running breast feed to the other side, as an edit event.
///
/// Bottles, solids, ended entries and sleep have no side, so this is a no-op
/// for them; the notification only offers the action for breast feeds.
#[cfg(any(target_os = "android", test))]
pub fn switch_feed_side(paths: &WidgetPaths, entry_id: &str) -> Result<bool> {
    let repo = paths.open_repo()?;
    let Some(mut entry) = repo.entry(entry_id)? else {
        return Ok(false);
    };
    if entry.deleted || entry.ended_at.is_some() || entry.kind != EntryKind::Feed {
        return Ok(false);
    }
    let next = match entry.feed.kind {
        Some(FeedKind::BreastLeft) => FeedKind::BreastRight,
        Some(FeedKind::BreastRight) => FeedKind::BreastLeft,
        _ => return Ok(false),
    };
    entry.feed.kind = Some(next);
    entry.updated_at = now_millis();
    let key = repo.device_key()?;
    let mut clock = repo.clock(key.id())?;
    repo.record_new_entry(&key, &mut clock, &entry)?;
    Ok(true)
}

/// When the running feed started, if one is running.
///
/// The Android notification posts a chronometer from this timestamp, so the
/// elapsed time stays right even when the notification is shown late.
#[cfg(any(target_os = "android", test))]
pub fn running_feed_start(paths: &WidgetPaths) -> Result<Option<i64>> {
    Ok(paths
        .open_repo()?
        .running_feed()?
        .map(|entry| entry.occurred_at))
}

/// Stop the running feed by authoring an edit event.
///
/// This deliberately goes through `record_new_entry` instead of writing the
/// row: a stop tapped in the notification has to be an event like any other,
/// or it would exist on this device and nowhere else.
#[cfg(any(target_os = "android", test))]
pub fn stop_feed(paths: &WidgetPaths) -> Result<bool> {
    let Some(entry) = paths.open_repo()?.running_feed()? else {
        return Ok(false);
    };
    stop_timer_entry(paths, &entry.id)
}

/// When the running sleep started, if one is running.
///
/// Like the feed timer, the notification draws its chronometer from this
/// timestamp so the elapsed time is right even when the notification appears
/// late.
#[cfg(any(target_os = "android", test))]
pub fn running_sleep_start(paths: &WidgetPaths) -> Result<Option<i64>> {
    Ok(paths
        .open_repo()?
        .running_sleep()?
        .map(|entry| entry.occurred_at))
}

/// Stop the running sleep by authoring an edit event.
///
/// The wake-up action on the sleep notification takes this path, so a wake from
/// the notification replicates exactly like one logged in the app.
#[cfg(any(target_os = "android", test))]
pub fn stop_sleep(paths: &WidgetPaths) -> Result<bool> {
    let Some(entry) = paths.open_repo()?.running_sleep()? else {
        return Ok(false);
    };
    stop_timer_entry(paths, &entry.id)
}

/* --------------------------------------------------------------------- JNI */

/// Called from `app.littlelog.widget.WidgetBridge` — Kotlin's WorkManager
/// worker and broadcast receiver reach the Rust core through these two
/// functions, in a plain app process with no activity running.
#[cfg(target_os = "android")]
mod android {
    use std::path::PathBuf;

    use jni::objects::{JObject, JString};
    use jni::sys::{jboolean, jint, jlong, jstring};
    use jni::JNIEnv;

    use super::{
        active_timers_json, drain_and_refresh, refresh, running_feed_start, running_sleep_start,
        stop_feed, stop_sleep, stop_timer_entry, switch_feed_side, WidgetPaths,
    };

    /// Read a `String` argument, or return `None` (and let the caller bail).
    fn string_arg(env: &mut JNIEnv, value: &JString) -> Option<String> {
        env.get_string(value).ok().map(|text| text.into())
    }

    /// Drain queued taps into SQLite, refresh the widget state.
    /// Returns whether anything was logged.
    #[no_mangle]
    pub extern "system" fn Java_app_littlelog_widget_WidgetBridge_nativeDrain(
        mut env: JNIEnv,
        _this: JObject,
        files_dir: JString,
        tz_offset_minutes: jint,
    ) -> jboolean {
        let Some(dir) = string_arg(&mut env, &files_dir) else {
            return 0;
        };
        let paths = WidgetPaths::discover(PathBuf::from(dir));
        match drain_and_refresh(&paths, tz_offset_minutes) {
            Ok((report, _)) => jboolean::from(report.applied > 0),
            Err(_) => 0,
        }
    }

    /// Refresh the widget state file only — used when the app itself changed
    /// something and the widget just needs to catch up.
    #[no_mangle]
    pub extern "system" fn Java_app_littlelog_widget_WidgetBridge_nativeRefresh(
        mut env: JNIEnv,
        _this: JObject,
        files_dir: JString,
        tz_offset_minutes: jint,
    ) {
        let Some(dir) = string_arg(&mut env, &files_dir) else {
            return;
        };
        let paths = WidgetPaths::discover(PathBuf::from(dir));
        let _ = refresh(&paths, tz_offset_minutes);
    }

    /// Start of the running feed (epoch millis), or 0 when none is running.
    /// Called from `app.littlelog.feed.FeedBridge` so the notification can post
    /// a chronometer that started when the feed did.
    #[no_mangle]
    pub extern "system" fn Java_app_littlelog_feed_FeedBridge_nativeRunningStart(
        mut env: JNIEnv,
        _this: JObject,
        data_dir: JString,
    ) -> jlong {
        let Some(dir) = string_arg(&mut env, &data_dir) else {
            return 0;
        };
        let paths = WidgetPaths::new(PathBuf::from(dir));
        running_feed_start(&paths).ok().flatten().unwrap_or(0)
    }

    /// Stop the running feed from the notification action. Returns whether a
    /// feed was running.
    #[no_mangle]
    pub extern "system" fn Java_app_littlelog_feed_FeedBridge_nativeStop(
        mut env: JNIEnv,
        _this: JObject,
        data_dir: JString,
    ) -> jboolean {
        let Some(dir) = string_arg(&mut env, &data_dir) else {
            return 0;
        };
        let paths = WidgetPaths::new(PathBuf::from(dir));
        jboolean::from(stop_feed(&paths).unwrap_or(false))
    }

    /// Start of the running sleep (epoch millis), or 0 when none is running.
    /// Called from `app.littlelog.sleep.SleepBridge` so the sleep notification
    /// can post a chronometer that started when the sleep did.
    #[no_mangle]
    pub extern "system" fn Java_app_littlelog_sleep_SleepBridge_nativeRunningStart(
        mut env: JNIEnv,
        _this: JObject,
        data_dir: JString,
    ) -> jlong {
        let Some(dir) = string_arg(&mut env, &data_dir) else {
            return 0;
        };
        let paths = WidgetPaths::new(PathBuf::from(dir));
        running_sleep_start(&paths).ok().flatten().unwrap_or(0)
    }

    /// Wake the baby from the notification action. Returns whether a sleep was
    /// running.
    #[no_mangle]
    pub extern "system" fn Java_app_littlelog_sleep_SleepBridge_nativeStop(
        mut env: JNIEnv,
        _this: JObject,
        data_dir: JString,
    ) -> jboolean {
        let Some(dir) = string_arg(&mut env, &data_dir) else {
            return 0;
        };
        let paths = WidgetPaths::new(PathBuf::from(dir));
        jboolean::from(stop_sleep(&paths).unwrap_or(false))
    }

    /// Every running timer as JSON, for `app.littlelog.timer.TimerBridge`.
    /// This is the notification's headless source of truth.
    #[no_mangle]
    pub extern "system" fn Java_app_littlelog_timer_TimerBridge_nativeActiveTimers(
        mut env: JNIEnv,
        _this: JObject,
        data_dir: JString,
    ) -> jstring {
        let Some(dir) = string_arg(&mut env, &data_dir) else {
            return std::ptr::null_mut();
        };
        let paths = WidgetPaths::new(PathBuf::from(dir));
        let json = active_timers_json(&paths).unwrap_or_else(|error| {
            eprintln!("timer notification: could not read open entries: {error}");
            "[]".to_string()
        });
        env.new_string(json)
            .map(|value| value.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// Stop exactly the entry the notification was showing. Returns whether it
    /// was still running.
    #[no_mangle]
    pub extern "system" fn Java_app_littlelog_timer_TimerBridge_nativeStopTimer(
        mut env: JNIEnv,
        _this: JObject,
        data_dir: JString,
        entry_id: JString,
    ) -> jboolean {
        let (Some(dir), Some(entry)) = (
            string_arg(&mut env, &data_dir),
            string_arg(&mut env, &entry_id),
        ) else {
            return 0;
        };
        let paths = WidgetPaths::new(PathBuf::from(dir));
        jboolean::from(stop_timer_entry(&paths, &entry).unwrap_or(false))
    }

    /// Flip a running breast feed to the other side. Returns whether it changed.
    #[no_mangle]
    pub extern "system" fn Java_app_littlelog_timer_TimerBridge_nativeSwitchFeedSide(
        mut env: JNIEnv,
        _this: JObject,
        data_dir: JString,
        entry_id: JString,
    ) -> jboolean {
        let (Some(dir), Some(entry)) = (
            string_arg(&mut env, &data_dir),
            string_arg(&mut env, &entry_id),
        ) else {
            return 0;
        };
        let paths = WidgetPaths::new(PathBuf::from(dir));
        jboolean::from(switch_feed_side(&paths, &entry).unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use little_log_core::model::{Entry, EntryKind, EventKind, FeedKind};
    use little_log_core::pending::{append, PendingAction, WidgetAction};

    #[test]
    fn a_drained_tap_reaches_the_database_and_the_widget_state_file() {
        let root = std::env::temp_dir().join(format!("little-log-widget-{}", now_millis()));
        let paths = WidgetPaths::new(&root);
        paths.publish_root().unwrap();
        {
            let repo = paths.open_repo().unwrap();
            repo.ensure_baby("Baby").unwrap();
        }

        append(
            &paths.queue(),
            PendingAction::new(WidgetAction::BreastLeft, now_millis()),
        )
        .unwrap();

        let (report, state) = drain_and_refresh(&paths, 0).unwrap();
        assert_eq!(report.applied, 1);
        assert_eq!(state.feeds_today, 1);
        assert!(state.last_feed_at.is_some());

        let written = std::fs::read_to_string(paths.state()).unwrap();
        assert!(written.contains("\"feeds_today\":1"));
        assert!(!paths.queue().exists(), "the queue is cleared once applied");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stop_from_the_notification_authors_an_edit() {
        let root = std::env::temp_dir().join(format!("little-log-feed-timer-{}", now_millis()));
        let paths = WidgetPaths::new(&root);
        let started_at = now_millis() - 5 * 60_000;
        let feed_id;

        {
            let repo = paths.open_repo().unwrap();
            let baby_id = repo.ensure_baby("Baby").unwrap().id;
            let key = repo.device_key().unwrap();
            let mut clock = repo.clock(key.id()).unwrap();

            let mut feed = Entry::new(EntryKind::Feed, started_at);
            feed.baby_id = baby_id;
            feed.feed.kind = Some(FeedKind::BreastLeft);
            feed_id = repo.record_new_entry(&key, &mut clock, &feed).unwrap().id;
        }

        assert_eq!(running_feed_start(&paths).unwrap(), Some(started_at));
        assert!(stop_feed(&paths).unwrap());
        assert!(
            !stop_feed(&paths).unwrap(),
            "a second stop has nothing to do"
        );

        let repo = paths.open_repo().unwrap();
        assert!(repo.running_feed().unwrap().is_none());
        let events = repo.log().for_entry(&feed_id).unwrap();
        assert_eq!(events.len(), 2, "start and stop are separate events");
        assert_eq!(events[0].kind, EventKind::Create);
        assert_eq!(events[1].kind, EventKind::Edit);
        assert!(repo.entry(&feed_id).unwrap().unwrap().ended_at.is_some());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_notification_sees_the_open_timer_and_its_child() {
        let root = std::env::temp_dir().join(format!("little-log-timers-{}", now_millis()));
        let paths = WidgetPaths::new(&root);
        let started_at = now_millis() - 7 * 60_000;

        {
            let repo = paths.open_repo().unwrap();
            let baby_id = repo.ensure_baby("Ada").unwrap().id;
            let key = repo.device_key().unwrap();
            let mut clock = repo.clock(key.id()).unwrap();
            let mut feed = Entry::new(EntryKind::Feed, started_at);
            feed.baby_id = baby_id;
            feed.feed.kind = Some(FeedKind::BreastLeft);
            repo.record_new_entry(&key, &mut clock, &feed).unwrap();
        }

        let json = active_timers_json(&paths).unwrap();
        assert!(json.contains("\"baby_name\":\"Ada\""), "{json}");
        assert!(json.contains("\"feed_kind\":\"breast_left\""), "{json}");
        assert!(
            json.contains(&format!("\"started_at\":{started_at}")),
            "{json}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn switching_side_authors_an_edit_and_keeps_the_entry_open() {
        let root = std::env::temp_dir().join(format!("little-log-switch-{}", now_millis()));
        let paths = WidgetPaths::new(&root);
        let feed_id;

        {
            let repo = paths.open_repo().unwrap();
            let baby_id = repo.ensure_baby("Ada").unwrap().id;
            let key = repo.device_key().unwrap();
            let mut clock = repo.clock(key.id()).unwrap();
            let mut feed = Entry::new(EntryKind::Feed, now_millis() - 3 * 60_000);
            feed.baby_id = baby_id;
            feed.feed.kind = Some(FeedKind::BreastLeft);
            feed_id = repo.record_new_entry(&key, &mut clock, &feed).unwrap().id;
        }

        assert!(switch_feed_side(&paths, &feed_id).unwrap());
        assert!(switch_feed_side(&paths, &feed_id).unwrap());

        let repo = paths.open_repo().unwrap();
        let stored = repo.entry(&feed_id).unwrap().unwrap();
        assert_eq!(stored.feed.kind, Some(FeedKind::BreastLeft));
        assert!(
            stored.ended_at.is_none(),
            "switching does not stop the feed"
        );
        assert_eq!(
            repo.log().for_entry(&feed_id).unwrap().len(),
            3,
            "start plus two switches are events"
        );

        // A bottle has no side to switch, and sleep is not a feed.
        assert!(!switch_feed_side(&paths, "missing").unwrap());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stop_targets_the_entry_the_notification_showed() {
        let root = std::env::temp_dir().join(format!("little-log-stop-entry-{}", now_millis()));
        let paths = WidgetPaths::new(&root);
        let older;
        let newer;

        {
            let repo = paths.open_repo().unwrap();
            let baby_id = repo.ensure_baby("Ada").unwrap().id;
            let key = repo.device_key().unwrap();
            let mut clock = repo.clock(key.id()).unwrap();

            let mut first = Entry::new(EntryKind::Feed, now_millis() - 10 * 60_000);
            first.baby_id = baby_id.clone();
            first.feed.kind = Some(FeedKind::BreastLeft);
            older = repo.record_new_entry(&key, &mut clock, &first).unwrap().id;

            let mut second = Entry::new(EntryKind::Feed, now_millis() - 2 * 60_000);
            second.baby_id = baby_id;
            second.feed.kind = Some(FeedKind::BreastRight);
            newer = repo.record_new_entry(&key, &mut clock, &second).unwrap().id;
        }

        // The stale notification for the older start must not stop the newer
        // feed; it stops exactly the entry it was showing.
        assert!(stop_timer_entry(&paths, &older).unwrap());
        let repo = paths.open_repo().unwrap();
        assert!(repo.entry(&older).unwrap().unwrap().ended_at.is_some());
        assert!(repo.entry(&newer).unwrap().unwrap().ended_at.is_none());
        assert_eq!(repo.running_feed().unwrap().unwrap().id, newer);
        assert!(
            !stop_timer_entry(&paths, &older).unwrap(),
            "already stopped"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_wake_from_the_notification_authors_an_edit() {
        let root = std::env::temp_dir().join(format!("little-log-sleep-timer-{}", now_millis()));
        let paths = WidgetPaths::new(&root);
        let started_at = now_millis() - 42 * 60_000;
        let sleep_id;

        {
            let repo = paths.open_repo().unwrap();
            let baby_id = repo.ensure_baby("Baby").unwrap().id;
            let key = repo.device_key().unwrap();
            let mut clock = repo.clock(key.id()).unwrap();

            let mut sleep = Entry::new(EntryKind::Sleep, started_at);
            sleep.baby_id = baby_id;
            sleep_id = repo.record_new_entry(&key, &mut clock, &sleep).unwrap().id;
        }

        assert_eq!(running_sleep_start(&paths).unwrap(), Some(started_at));
        assert!(stop_sleep(&paths).unwrap());
        assert!(
            !stop_sleep(&paths).unwrap(),
            "a second wake has nothing to do"
        );

        let repo = paths.open_repo().unwrap();
        assert!(repo.running_sleep().unwrap().is_none());
        let events = repo.log().for_entry(&sleep_id).unwrap();
        assert_eq!(events.len(), 2, "start and wake are separate events");
        assert_eq!(events[0].kind, EventKind::Create);
        assert_eq!(events[1].kind, EventKind::Edit);
        assert!(repo.entry(&sleep_id).unwrap().unwrap().ended_at.is_some());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_follows_the_marker_the_app_writes() {
        let files_dir = std::env::temp_dir().join(format!("little-log-files-{}", now_millis()));
        let data_dir = files_dir.join("nested/data");
        std::fs::create_dir_all(&files_dir).unwrap();

        // Before the app has ever run: fall back to the files directory.
        assert_eq!(WidgetPaths::discover(&files_dir).data_dir, files_dir);

        WidgetPaths::new(&data_dir).publish_root().unwrap();
        std::fs::write(
            files_dir.join("widget_root.txt"),
            data_dir.display().to_string(),
        )
        .unwrap();
        assert_eq!(WidgetPaths::discover(&files_dir).data_dir, data_dir);

        let _ = std::fs::remove_dir_all(&files_dir);
    }

    #[test]
    fn refresh_reports_a_running_sleep() {
        let root = std::env::temp_dir().join(format!("little-log-sleep-{}", now_millis()));
        let paths = WidgetPaths::new(&root);
        let repo = paths.open_repo().unwrap();
        let baby_id = repo.ensure_baby("Baby").unwrap().id;
        let mut sleep = Entry::new(EntryKind::Sleep, now_millis() - 42 * 60_000);
        sleep.ended_at = None;
        sleep.baby_id = baby_id;
        repo.force_entry(&sleep).unwrap();

        let state = refresh(&paths, 0).unwrap();
        assert_eq!(state.sleeping_since, Some(sleep.occurred_at));
        assert_eq!(state.feeds_today, 0);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_tap_before_the_app_has_ever_run_still_lands_somewhere_sane() {
        let files_dir = std::env::temp_dir().join(format!("little-log-first-{}", now_millis()));
        std::fs::create_dir_all(&files_dir).unwrap();
        let paths = WidgetPaths::discover(&files_dir);

        append(
            &paths.queue(),
            PendingAction::new(WidgetAction::Bottle, now_millis()),
        )
        .unwrap();
        let (report, _) = drain_and_refresh(&paths, 0).unwrap();
        assert_eq!(report.applied, 1);

        // Feeding a bottle with no amount is valid; the entry is stored.
        let repo = paths.open_repo().unwrap();
        let entry = repo.entries().unwrap().into_iter().next().unwrap();
        assert_eq!(entry.kind, EntryKind::Feed);
        assert_eq!(entry.feed.kind, Some(FeedKind::Bottle));

        let _ = std::fs::remove_dir_all(&files_dir);
    }
}
