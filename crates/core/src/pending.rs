//! The Android widget's one-tap path.
//!
//! A tap on the widget can't open the app, so the Kotlin receiver appends an
//! action to a small JSON queue in the app's files directory. The app drains
//! that queue on launch and from a WorkManager worker, which calls into
//! [`drain_into`] through JNI.
//!
//! Two properties make this safe:
//!   * each action carries a UUIDv7 id from the widget, so applying it twice
//!     (a re-delivered broadcast, a retried worker) cannot duplicate an entry —
//!     the id *is* the entry id, and writes are idempotent upserts;
//!   * the queue is only deleted after the entries are stored, so a crash
//!     between the two loses nothing.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::model::{DiaperType, Entry, EntryKind, FeedKind, Millis};
use crate::repo::Repo;
use crate::validate::now_millis;

/// The six one-tap buttons on the home-screen widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetAction {
    BreastLeft,
    BreastRight,
    Bottle,
    Pee,
    Poop,
    /// Start a sleep, or wake the baby up if one is running.
    SleepToggle,
}

impl WidgetAction {
    /// The id Kotlin writes into the queue.
    pub const fn as_str(self) -> &'static str {
        match self {
            WidgetAction::BreastLeft => "breast_left",
            WidgetAction::BreastRight => "breast_right",
            WidgetAction::Bottle => "bottle",
            WidgetAction::Pee => "pee",
            WidgetAction::Poop => "poop",
            WidgetAction::SleepToggle => "sleep_toggle",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "breast_left" => Ok(WidgetAction::BreastLeft),
            "breast_right" => Ok(WidgetAction::BreastRight),
            "bottle" => Ok(WidgetAction::Bottle),
            "pee" => Ok(WidgetAction::Pee),
            "poop" => Ok(WidgetAction::Poop),
            "sleep_toggle" => Ok(WidgetAction::SleepToggle),
            other => Err(CoreError::UnknownEnum {
                enum_name: "WidgetAction",
                value: other.to_string(),
            }),
        }
    }
}

/// One queued tap. `id` becomes the entry id, which is what makes draining
/// idempotent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingAction {
    pub id: String,
    pub action: WidgetAction,
    /// When the button was tapped, ms since epoch.
    pub at: Millis,
}

impl PendingAction {
    pub fn new(action: WidgetAction, at: Millis) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            action,
            at,
        }
    }
}

/// What a drain did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrainReport {
    /// Entries written (including idempotent no-ops).
    pub applied: usize,
    /// Actions that were already stored and changed nothing.
    pub skipped: usize,
}

/// Read the queue without clearing it (used by tests and diagnostics).
pub fn read(path: &Path) -> Result<Vec<PendingAction>> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(Vec::new()),
        Ok(text) => serde_json::from_str(&text)
            .map_err(|error| CoreError::data(format!("pending actions are unreadable: {error}"))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(CoreError::io(path.display().to_string(), error)),
    }
}

/// Append one tap (Kotlin calls this from the widget receiver).
pub fn append(path: &Path, action: PendingAction) -> Result<()> {
    let mut queue = read(path)?;
    queue.push(action);
    write(path, &queue)
}

fn write(path: &Path, queue: &[PendingAction]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CoreError::io(parent.display().to_string(), error))?;
    }
    let text = serde_json::to_string(queue).map_err(|error| CoreError::data(error.to_string()))?;
    // Write beside the target, then rename: readers never see half a queue.
    let temp = path.with_extension(format!("tmp{}", now_millis()));
    std::fs::write(&temp, text)
        .map_err(|error| CoreError::io(temp.display().to_string(), error))?;
    std::fs::rename(&temp, path).map_err(|error| CoreError::io(path.display().to_string(), error))
}

/// How many applied action ids we remember (enough for any retry in practice).
const APPLIED_KEEP: usize = 200;

/// Where the ledger of already-applied taps lives.
pub fn applied_ledger_path(queue_path: &Path) -> std::path::PathBuf {
    queue_path.with_file_name("applied_actions.json")
}

/// Apply every queued tap to the database, then clear the queue.
///
/// A tap that was already applied (a re-delivered broadcast, a retried worker)
/// is skipped, so one tap is always exactly one entry.
pub fn drain_into(path: &Path, repo: &Repo, caregiver_id: Option<&str>) -> Result<DrainReport> {
    let queue = read(path)?;
    if queue.is_empty() {
        return Ok(DrainReport::default());
    }

    let now = now_millis();
    let baby_id = repo.baby()?.and_then(|baby| baby.id);
    let ledger_path = applied_ledger_path(path);
    let mut applied_ids = read_ids(&ledger_path)?;
    let mut report = DrainReport::default();

    for pending in &queue {
        if applied_ids.iter().any(|id| id == &pending.id) {
            report.record(false);
            continue;
        }
        let applied = apply(repo, pending, now, baby_id.as_deref(), caregiver_id)?;
        report.record(applied);
        applied_ids.push(pending.id.clone());
    }

    if applied_ids.len() > APPLIED_KEEP {
        let excess = applied_ids.len() - APPLIED_KEEP;
        applied_ids.drain(..excess);
    }
    write_ids(&ledger_path, &applied_ids)?;

    // Only now is it safe to forget the taps: every entry is stored and the
    // ledger remembers their ids.
    remove(path)?;
    Ok(report)
}

fn read_ids(path: &Path) -> Result<Vec<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(Vec::new()),
        Ok(text) => serde_json::from_str(&text)
            .map_err(|error| CoreError::data(format!("applied ledger is unreadable: {error}"))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(CoreError::io(path.display().to_string(), error)),
    }
}

fn write_ids(path: &Path, ids: &[String]) -> Result<()> {
    let text = serde_json::to_string(ids).map_err(|error| CoreError::data(error.to_string()))?;
    std::fs::write(path, text).map_err(|error| CoreError::io(path.display().to_string(), error))
}

fn apply(
    repo: &Repo,
    pending: &PendingAction,
    now: Millis,
    baby_id: Option<&str>,
    caregiver_id: Option<&str>,
) -> Result<bool> {
    let occurred_at = if pending.at > 0 { pending.at } else { now };
    let mut is_new = true;
    let mut entry = match pending.action {
        WidgetAction::BreastLeft | WidgetAction::BreastRight | WidgetAction::Bottle => {
            let mut entry = Entry::new(EntryKind::Feed, occurred_at);
            entry.feed_kind = Some(match pending.action {
                WidgetAction::BreastLeft => FeedKind::BreastLeft,
                WidgetAction::BreastRight => FeedKind::BreastRight,
                _ => FeedKind::Bottle,
            });
            // One tap can't ask "how many minutes / ml" — leave the amount open
            // so nothing is invented; it can be filled in from the app.
            entry
        }
        WidgetAction::Pee | WidgetAction::Poop => {
            let mut entry = Entry::new(EntryKind::Diaper, occurred_at);
            entry.diaper_type = Some(match pending.action {
                WidgetAction::Pee => DiaperType::Pee,
                _ => DiaperType::Poop,
            });
            entry
        }
        WidgetAction::SleepToggle => match repo.running_sleep()? {
            Some(running) => {
                is_new = false;
                let mut entry = running;
                // A same-millisecond wake tap still has to be a valid sleep.
                entry.ended_at = Some(occurred_at.max(entry.occurred_at + 60_000));
                entry
            }
            None => Entry::new(EntryKind::Sleep, occurred_at),
        },
    };

    if is_new {
        entry.id = pending.id.clone();
    }
    entry.baby_id = baby_id.map(str::to_string);
    entry.caregiver_id = caregiver_id.map(str::to_string);
    if is_new {
        entry.created_at = occurred_at;
    }
    // Both clocks come from the tap, never from "now": replaying the same tap
    // produces an identical row, which last-write-wins then skips.
    entry.updated_at = occurred_at;

    repo.save_remote_entry(&entry)
}

fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CoreError::io(path.display().to_string(), error)),
    }
}

/// The numbers the widget renders: "Last fed 1h 20m ago" and "Sleeping 42m".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetState {
    pub last_feed_at: Option<Millis>,
    pub sleeping_since: Option<Millis>,
    pub feeds_today: i64,
    pub diapers_today: i64,
}

/// Compute the widget's state. `tz_offset_minutes` is the device's UTC offset,
/// so "today" means the parent's today.
pub fn widget_state(repo: &Repo, now: Millis, tz_offset_minutes: i32) -> Result<WidgetState> {
    let offset = i64::from(tz_offset_minutes) * 60_000;
    let day = 86_400_000_i64;
    let day_start = ((now + offset).div_euclid(day)) * day - offset;
    let day_end = day_start + day;

    let mut state = WidgetState::default();
    let mut feeds_today = 0;
    let mut diapers_today = 0;

    for entry in repo.entries_since(None)? {
        if entry.deleted {
            continue;
        }
        if entry.kind == EntryKind::Sleep && entry.ended_at.is_none() {
            state.sleeping_since = Some(entry.occurred_at);
        }
        if entry.kind == EntryKind::Feed {
            state.last_feed_at = Some(
                state
                    .last_feed_at
                    .map_or(entry.occurred_at, |last| last.max(entry.occurred_at)),
            );
            if (day_start..day_end).contains(&entry.occurred_at) {
                feeds_today += 1;
            }
        }
        if entry.kind == EntryKind::Diaper && (day_start..day_end).contains(&entry.occurred_at) {
            diapers_today += 1;
        }
    }

    state.feeds_today = feeds_today;
    state.diapers_today = diapers_today;
    Ok(state)
}

/// Write the widget state where Kotlin can read it.
pub fn write_widget_state(path: &Path, state: &WidgetState) -> Result<()> {
    let text = serde_json::to_string(state).map_err(|error| CoreError::data(error.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CoreError::io(parent.display().to_string(), error))?;
    }
    std::fs::write(path, text).map_err(|error| CoreError::io(path.display().to_string(), error))
}

impl DrainReport {
    fn record(&mut self, applied: bool) {
        if applied {
            self.applied += 1;
        } else {
            self.skipped += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DiaperType, FeedKind};

    fn temp_queue(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("little-log-queue-{name}-{}", now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("pending_actions.json")
    }

    #[test]
    fn the_queue_round_trips_and_drains_empty() {
        let path = temp_queue("round-trip");
        let repo = Repo::open_in_memory().unwrap();

        append(&path, PendingAction::new(WidgetAction::BreastLeft, 1_000)).unwrap();
        append(&path, PendingAction::new(WidgetAction::Pee, 2_000)).unwrap();
        assert_eq!(read(&path).unwrap().len(), 2);

        let report = drain_into(&path, &repo, None).unwrap();
        assert_eq!(report.applied, 2);
        assert_eq!(report.skipped, 0);
        assert!(read(&path).unwrap().is_empty());
        assert_eq!(repo.count_entries().unwrap(), 2);

        // Draining an empty queue is a no-op.
        assert_eq!(
            drain_into(&path, &repo, None).unwrap(),
            DrainReport::default()
        );
    }

    #[test]
    fn one_tap_logs_the_right_entry_without_inventing_numbers() {
        let path = temp_queue("mapping");
        let repo = Repo::open_in_memory().unwrap();
        let baby = repo.ensure_baby("Baby").unwrap();

        for action in [
            WidgetAction::BreastLeft,
            WidgetAction::BreastRight,
            WidgetAction::Bottle,
            WidgetAction::Pee,
            WidgetAction::Poop,
        ] {
            append(&path, PendingAction::new(action, 10_000)).unwrap();
        }
        let report = drain_into(&path, &repo, Some("caregiver-1")).unwrap();
        assert_eq!(report.applied, 5);

        let entries = repo.entries().unwrap();
        assert_eq!(entries.len(), 5);
        for entry in &entries {
            assert_eq!(entry.baby_id, baby.id);
            assert_eq!(entry.caregiver_id.as_deref(), Some("caregiver-1"));
            // No amount, no attributes: a single tap knows nothing more.
            assert_eq!(entry.amount, None);
            assert_eq!(entry.pee_shade, None);
            assert_eq!(entry.poop_color, None);
        }
        let feeds: Vec<Option<FeedKind>> = entries
            .iter()
            .filter(|entry| entry.kind == EntryKind::Feed)
            .map(|entry| entry.feed_kind)
            .collect();
        assert_eq!(
            feeds,
            vec![
                Some(FeedKind::Bottle),
                Some(FeedKind::BreastRight),
                Some(FeedKind::BreastLeft)
            ]
        );
        let diapers: Vec<Option<DiaperType>> = entries
            .iter()
            .filter(|entry| entry.kind == EntryKind::Diaper)
            .map(|entry| entry.diaper_type)
            .collect();
        assert_eq!(diapers.len(), 2);
    }

    #[test]
    fn the_sleep_toggle_starts_and_ends_a_sleep() {
        let path = temp_queue("sleep");
        let repo = Repo::open_in_memory().unwrap();

        append(&path, PendingAction::new(WidgetAction::SleepToggle, 5_000)).unwrap();
        drain_into(&path, &repo, None).unwrap();
        let running = repo.running_sleep().unwrap().expect("sleeping");
        assert_eq!(running.occurred_at, 5_000);
        assert!(running.ended_at.is_none());

        append(
            &path,
            PendingAction::new(WidgetAction::SleepToggle, 5_000 + 40 * 60_000),
        )
        .unwrap();
        drain_into(&path, &repo, None).unwrap();
        assert!(repo.running_sleep().unwrap().is_none());

        let sleeps: Vec<Entry> = repo
            .entries()
            .unwrap()
            .into_iter()
            .filter(|entry| entry.kind == EntryKind::Sleep)
            .collect();
        assert_eq!(sleeps.len(), 1);
        assert_eq!(sleeps[0].ended_at, Some(5_000 + 40 * 60_000));
        assert_eq!(sleeps[0].duration_ms(0), 40 * 60_000);
    }

    #[test]
    fn replaying_a_tap_cannot_duplicate_it() {
        let path = temp_queue("idempotent");
        let repo = Repo::open_in_memory().unwrap();
        let tap = PendingAction::new(WidgetAction::Bottle, 9_000);

        append(&path, tap.clone()).unwrap();
        drain_into(&path, &repo, None).unwrap();
        assert_eq!(repo.count_entries().unwrap(), 1);

        // The same tap arrives again (retried broadcast / worker re-run) …
        append(&path, tap).unwrap();
        let report = drain_into(&path, &repo, None).unwrap();
        assert_eq!(report.applied, 0);
        assert_eq!(report.skipped, 1);
        assert_eq!(repo.count_entries().unwrap(), 1);

        // … and a fresh tap still works.
        append(&path, PendingAction::new(WidgetAction::Bottle, 10_000)).unwrap();
        assert_eq!(drain_into(&path, &repo, None).unwrap().applied, 1);
        assert_eq!(repo.count_entries().unwrap(), 2);
    }

    #[test]
    fn a_sleep_ending_before_it_started_is_clamped_to_a_minute() {
        let path = temp_queue("clamp");
        let repo = Repo::open_in_memory().unwrap();

        append(
            &path,
            PendingAction::new(WidgetAction::SleepToggle, 100_000),
        )
        .unwrap();
        drain_into(&path, &repo, None).unwrap();
        // Same-millisecond wake-up: the entry must still be valid.
        append(
            &path,
            PendingAction::new(WidgetAction::SleepToggle, 100_000),
        )
        .unwrap();
        let report = drain_into(&path, &repo, None).unwrap();
        assert_eq!(report.applied, 1);

        let sleep = repo.entries().unwrap().into_iter().next().unwrap();
        assert_eq!(sleep.ended_at, Some(160_000));
    }

    #[test]
    fn unknown_actions_in_the_queue_are_rejected_not_guessed() {
        let path = temp_queue("unknown");
        std::fs::write(&path, r#"[{"id":"x","action":"teleport","at":1}]"#).unwrap();
        let error = read(&path).unwrap_err();
        assert!(error.to_string().contains("pending actions"));
    }

    #[test]
    fn the_widget_state_reports_last_feed_sleeping_and_today_counts() {
        let repo = Repo::open_in_memory().unwrap();
        let now = 1_760_000_000_000; // 2025-10-09T08:26:40Z
        let day = 86_400_000;

        let mut feed = Entry::new(EntryKind::Feed, now - 30 * 60_000);
        feed.feed_kind = Some(FeedKind::Bottle);
        repo.force_entry(&feed).unwrap();
        let mut yesterday = Entry::new(EntryKind::Feed, now - day);
        yesterday.feed_kind = Some(FeedKind::Bottle);
        repo.force_entry(&yesterday).unwrap();
        let mut diaper = Entry::new(EntryKind::Diaper, now - 60 * 60_000);
        diaper.diaper_type = Some(DiaperType::Pee);
        repo.force_entry(&diaper).unwrap();
        let mut sleep = Entry::new(EntryKind::Sleep, now - 42 * 60_000);
        sleep.ended_at = None;
        repo.force_entry(&sleep).unwrap();

        let state = widget_state(&repo, now, 0).unwrap();
        assert_eq!(state.last_feed_at, Some(now - 30 * 60_000));
        assert_eq!(state.sleeping_since, Some(now - 42 * 60_000));
        assert_eq!(state.feeds_today, 1);
        assert_eq!(state.diapers_today, 1);

        // In UTC+2 the same moment is 10:26, so the "yesterday" feed is still
        // yesterday and the counts hold.
        let state = widget_state(&repo, now, 120).unwrap();
        assert_eq!(state.feeds_today, 1);

        // Written where Kotlin can read it.
        let path = std::env::temp_dir().join(format!("little-log-widget-{now}.json"));
        write_widget_state(&path, &state).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"feeds_today\":1"));
        let _ = std::fs::remove_file(&path);
    }
}
