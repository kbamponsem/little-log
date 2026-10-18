//! The widget's headless path: drain queued taps, refresh the widget state.
//!
//! Kotlin owns the widget surface; this module owns the data. Both the app
//! (`AppState`) and the Android `WidgetBridge` (called from a WorkManager
//! worker, with no UI and no Tauri runtime) go through here, so there is
//! exactly one definition of "where the widget reads and writes".

use std::path::{Path, PathBuf};

use little_log_core::pending::{self, DrainReport, WidgetState};
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

/* --------------------------------------------------------------------- JNI */

/// Called from `app.littlelog.widget.WidgetBridge` — Kotlin's WorkManager
/// worker and broadcast receiver reach the Rust core through these two
/// functions, in a plain app process with no activity running.
#[cfg(target_os = "android")]
mod android {
    use std::path::PathBuf;

    use jni::objects::{JObject, JString};
    use jni::sys::{jboolean, jint};
    use jni::JNIEnv;

    use super::{drain_and_refresh, refresh, WidgetPaths};

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use little_log_core::model::{Entry, EntryKind, FeedKind};
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
        repo.ensure_baby("Baby").unwrap();
        let mut sleep = Entry::new(EntryKind::Sleep, now_millis() - 42 * 60_000);
        sleep.ended_at = None;
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
        assert_eq!(entry.feed_kind, Some(FeedKind::Bottle));

        let _ = std::fs::remove_dir_all(&files_dir);
    }
}
