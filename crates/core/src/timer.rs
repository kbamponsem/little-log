//! Running timers derived from open entries.
//!
//! A timer is not stored anywhere of its own: an entry with no `ended_at` *is*
//! the timer, and its `occurred_at` is the start. Both the Android notification
//! and the Rust side derive this list from SQLite, so a start, edit or stop from
//! any device produces the same answer without the notification holding state.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::{EntryKind, FeedKind, Millis};
use crate::repo::Repo;

/// The two kinds of activity that run a clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerKind {
    Feed,
    Sleep,
}

impl TimerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            TimerKind::Feed => "feed",
            TimerKind::Sleep => "sleep",
        }
    }

    /// The timer kind an entry runs, if it runs one.
    pub fn of_entry(kind: EntryKind) -> Option<Self> {
        match kind {
            EntryKind::Feed => Some(TimerKind::Feed),
            EntryKind::Sleep => Some(TimerKind::Sleep),
            EntryKind::Diaper | EntryKind::Mood => None,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "feed" => Some(TimerKind::Feed),
            "sleep" => Some(TimerKind::Sleep),
            _ => None,
        }
    }
}

/// One running activity, ready for the Android notification.
///
/// `baby_id` scopes the notification: the same device may have one running feed
/// and one running sleep for each child. `feed_kind` lets the feed notification
/// offer "Switch side" for a breast feed and omit it for a bottle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActiveTimer {
    pub kind: TimerKind,
    /// The open entry; a stop or edit targets exactly this row.
    pub entry_id: String,
    pub baby_id: Option<String>,
    /// The child's profile name, or empty when the log has no child yet.
    pub baby_name: String,
    pub started_at: Millis,
    pub feed_kind: Option<FeedKind>,
}

/// Every running feed and sleep, one per child per kind.
///
/// If an older version left more than one open entry for the same child and
/// kind, the newest start wins — the same rule `running_feed`/`running_sleep`
/// already use, so the notification and the in-app "stop" agree on which entry
/// is running.
pub fn active_timers(repo: &Repo) -> Result<Vec<ActiveTimer>> {
    let babies = repo.babies()?;
    let current = repo.current_baby()?;

    // A legacy entry can have no `baby_id`. It belongs to the child this device
    // is looking at; with no child at all the title simply omits the name.
    let fallback_name = current
        .as_ref()
        .map(|baby| baby.name.clone())
        .or_else(|| babies.first().map(|baby| baby.name.clone()))
        .unwrap_or_default();
    let names: HashMap<&str, &str> = babies
        .iter()
        .filter_map(|baby| baby.id.as_deref().map(|id| (id, baby.name.as_str())))
        .collect();

    // One slot per (child, kind). Later starts replace earlier ones in place.
    let mut slots: HashMap<(Option<String>, TimerKind), ActiveTimer> = HashMap::new();
    for entry in repo.entries()? {
        if entry.deleted || entry.ended_at.is_some() {
            continue;
        }
        let kind = match entry.kind {
            EntryKind::Feed => TimerKind::Feed,
            EntryKind::Sleep => TimerKind::Sleep,
            // A diaper or mood row never runs a clock, even if a malformed row
            // has no end time.
            EntryKind::Diaper | EntryKind::Mood => continue,
        };

        let baby_name = entry
            .baby_id
            .as_deref()
            .and_then(|id| names.get(id).copied())
            .map(str::to_string)
            .unwrap_or_else(|| fallback_name.clone());
        let candidate = ActiveTimer {
            kind,
            entry_id: entry.id,
            baby_id: entry.baby_id,
            baby_name,
            started_at: entry.occurred_at,
            feed_kind: if kind == TimerKind::Feed {
                entry.feed.kind
            } else {
                None
            },
        };

        let slot = (candidate.baby_id.clone(), kind);
        match slots.get(&slot) {
            // Tie-break on the id so two rows with the same timestamp still
            // resolve to one deterministic timer.
            Some(existing)
                if (existing.started_at, &existing.entry_id)
                    >= (candidate.started_at, &candidate.entry_id) => {}
            _ => {
                slots.insert(slot, candidate);
            }
        }
    }

    let mut timers: Vec<ActiveTimer> = slots.into_values().collect();
    timers.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.baby_name.cmp(&right.baby_name))
            .then_with(|| left.started_at.cmp(&right.started_at))
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    Ok(timers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Baby;
    use crate::validate::now_millis;

    fn open_feed(baby_id: &str, at: Millis, side: FeedKind) -> crate::model::Entry {
        let mut entry = crate::model::Entry::new(EntryKind::Feed, at);
        entry.baby_id = Some(baby_id.to_string());
        entry.feed.kind = Some(side);
        entry
    }

    fn open_sleep(baby_id: &str, at: Millis) -> crate::model::Entry {
        let mut entry = crate::model::Entry::new(EntryKind::Sleep, at);
        entry.baby_id = Some(baby_id.to_string());
        entry
    }

    #[test]
    fn only_an_open_entry_is_a_timer() {
        let repo = Repo::open_in_memory().unwrap();
        let baby = repo.ensure_baby("Ada").unwrap();
        let baby_id = baby.id.unwrap();
        let now = now_millis();

        let open = open_feed(&baby_id, now - 5 * 60_000, FeedKind::BreastLeft);
        repo.force_entry(&open).unwrap();

        let mut stopped = open_feed(&baby_id, now - 30 * 60_000, FeedKind::Bottle);
        stopped.ended_at = Some(now - 20 * 60_000);
        repo.force_entry(&stopped).unwrap();

        let mut deleted = open_feed(&baby_id, now - 10 * 60_000, FeedKind::BreastRight);
        deleted.deleted = true;
        repo.force_entry(&deleted).unwrap();

        let timers = active_timers(&repo).unwrap();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].entry_id, open.id);
        assert_eq!(timers[0].baby_name, "Ada");
        assert_eq!(timers[0].feed_kind, Some(FeedKind::BreastLeft));
    }

    #[test]
    fn one_timer_per_baby_and_kind_is_the_latest_start() {
        let repo = Repo::open_in_memory().unwrap();
        let ada = repo.ensure_baby("Ada").unwrap().id.unwrap();
        let now = now_millis();

        let first = open_feed(&ada, now - 20 * 60_000, FeedKind::BreastLeft);
        repo.force_entry(&first).unwrap();
        let second = open_feed(&ada, now - 5 * 60_000, FeedKind::BreastRight);
        repo.force_entry(&second).unwrap();

        let timers = active_timers(&repo).unwrap();
        assert_eq!(timers.len(), 1, "one phone shows one feed timer per child");
        assert_eq!(timers[0].entry_id, second.id);
        assert_eq!(timers[0].started_at, second.occurred_at);
        assert_eq!(timers[0].feed_kind, Some(FeedKind::BreastRight));
    }

    #[test]
    fn siblings_and_the_two_kinds_are_separate_timers() {
        let repo = Repo::open_in_memory().unwrap();
        let ada = repo.ensure_baby("Ada").unwrap().id.unwrap();
        let bo = repo
            .save_baby(&Baby {
                id: None,
                name: "Bo".into(),
                ..Default::default()
            })
            .unwrap()
            .id
            .unwrap();
        let now = now_millis();

        repo.force_entry(&open_feed(&ada, now - 10 * 60_000, FeedKind::Bottle))
            .unwrap();
        repo.force_entry(&open_sleep(&ada, now - 40 * 60_000))
            .unwrap();
        repo.force_entry(&open_feed(&bo, now - 3 * 60_000, FeedKind::BreastLeft))
            .unwrap();

        let timers = active_timers(&repo).unwrap();
        assert_eq!(timers.len(), 3, "two feeds and one sleep");

        let ada_feed = timers
            .iter()
            .find(|timer| timer.kind == TimerKind::Feed && timer.baby_name == "Ada")
            .expect("Ada's feed");
        assert_eq!(ada_feed.feed_kind, Some(FeedKind::Bottle));
        assert!(timers.iter().any(|timer| {
            timer.kind == TimerKind::Feed && timer.baby_id.as_deref() == Some(bo.as_str())
        }));
        assert!(timers.iter().any(|timer| timer.kind == TimerKind::Sleep
            && timer.baby_id.as_deref() == Some(ada.as_str())));
    }

    #[test]
    fn an_entry_without_a_child_uses_the_current_child_name() {
        let repo = Repo::open_in_memory().unwrap();
        repo.ensure_baby("Ada").unwrap();
        let now = now_millis();

        let mut legacy = open_feed("ignored", now - 2 * 60_000, FeedKind::BreastLeft);
        legacy.baby_id = None;
        repo.force_entry(&legacy).unwrap();

        let timers = active_timers(&repo).unwrap();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].baby_name, "Ada");
        assert_eq!(timers[0].baby_id, None);
    }
}
