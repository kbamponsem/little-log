//! Sync primitives: last-write-wins merging and the composite pull cursor.
//!
//! Both the app and the server use these, so a conflict resolves the same way
//! on either side. Pushes are idempotent: entry ids are UUIDv7 generated on the
//! device and re-sending a row with an unchanged `updated_at` is a no-op.

use crate::model::{Entry, Millis};

/// `"<updated_at>:<id>"` — ordered exactly like `(updated_at, id)` in SQLite.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor {
    pub updated_at: Millis,
    pub id: String,
}

impl Cursor {
    pub fn new(updated_at: Millis, id: impl Into<String>) -> Self {
        Self {
            updated_at,
            id: id.into(),
        }
    }

    /// The cursor that points at `entry`.
    pub fn of(entry: &Entry) -> Self {
        Self::new(entry.updated_at, entry.id.clone())
    }

    /// The largest cursor in a page, i.e. the next `since` value.
    pub fn latest(entries: &[Entry]) -> Option<Self> {
        entries.iter().map(Cursor::of).max()
    }

    pub fn parse(value: &str) -> Option<Self> {
        let (updated_at, id) = value.split_once(':')?;
        Some(Self::new(updated_at.parse().ok()?, id))
    }

    pub fn as_string(&self) -> String {
        format!("{}:{}", self.updated_at, self.id)
    }

    /// True when `entry` is at or before this cursor (i.e. already seen).
    pub fn includes(&self, entry: &Entry) -> bool {
        (entry.updated_at, entry.id.as_str()) <= (self.updated_at, self.id.as_str())
    }
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_string())
    }
}

/// Last-write-wins. Returns the row to store, or `None` when the local copy
/// already wins.
///
/// Ties go to the incoming row: applying the same push twice is a no-op, which
/// makes retries safe.
pub fn merge(local: Option<&Entry>, incoming: &Entry) -> Option<Entry> {
    match local {
        None => Some(incoming.clone()),
        // Identical payload: nothing to write (makes retries cheap and honest).
        Some(local) if local == incoming => None,
        Some(local) if incoming.updated_at >= local.updated_at => Some(incoming.clone()),
        Some(_) => None,
    }
}

/// Result of merging a batch of pushed entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Rows that were newer than what we had (written).
    pub applied: usize,
    /// Rows we ignored because ours were newer.
    pub skipped: usize,
}

impl MergeReport {
    pub fn record(&mut self, applied: bool) {
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
    use crate::model::{EntryKind, FeedKind};

    fn entry(updated_at: Millis) -> Entry {
        let mut entry = Entry::new(EntryKind::Feed, updated_at);
        entry.updated_at = updated_at;
        entry.feed_kind = Some(FeedKind::Bottle);
        entry
    }

    #[test]
    fn a_new_id_is_always_accepted() {
        let incoming = entry(10);
        assert_eq!(merge(None, &incoming), Some(incoming));
    }

    #[test]
    fn the_newer_row_wins() {
        let local = entry(100);
        let incoming = entry(200);
        assert_eq!(merge(Some(&local), &incoming), Some(incoming));

        let stale = entry(50);
        assert_eq!(merge(Some(&local), &stale), None);
    }

    #[test]
    fn ties_favour_the_incoming_row_so_retries_are_idempotent() {
        let local = entry(100);
        let mut incoming = entry(100);
        incoming.amount = Some(120);

        let first = merge(Some(&local), &incoming).unwrap();
        assert_eq!(first.amount, Some(120));

        // Re-sending a byte-identical payload writes nothing at all.
        assert_eq!(merge(Some(&first), &incoming), None);

        // A tie with different content still favours the incoming row.
        let mut other = incoming.clone();
        other.note = Some("newer".into());
        assert_eq!(
            merge(Some(&first), &other).unwrap().note.as_deref(),
            Some("newer")
        );
    }

    #[test]
    fn deletes_propagate_like_any_other_update() {
        let live = entry(100);
        let mut tombstone = live.clone();
        tombstone.deleted = true;
        tombstone.updated_at = 150;

        let merged = merge(Some(&live), &tombstone).unwrap();
        assert!(merged.deleted);

        // A late edit of an old copy must not resurrect the entry.
        let mut resurrect = live.clone();
        resurrect.updated_at = 120;
        assert_eq!(merge(Some(&merged), &resurrect), None);
    }

    #[test]
    fn a_conflict_between_two_devices_resolves_the_same_way_both_directions() {
        let mut phone = entry(1_000);
        phone.note = Some("phone".into());
        let mut tablet = entry(2_000);
        tablet.note = Some("tablet".into());

        let after_phone_pull = merge(Some(&phone), &tablet).unwrap();
        let after_tablet_pull = merge(Some(&tablet), &phone);
        assert_eq!(after_phone_pull.note.as_deref(), Some("tablet"));
        assert_eq!(after_tablet_pull, None);
    }

    #[test]
    fn cursors_round_trip_and_order_like_sqlite() {
        let cursor = Cursor::new(1_760_000_000_000, "0192f0a1-2b3c-7d4e-8f5a-6b7c8d9e0f10");
        assert_eq!(Cursor::parse(&cursor.as_string()), Some(cursor.clone()));

        let mut entry = entry(cursor.updated_at);
        entry.id = cursor.id.clone();
        assert!(cursor.includes(&entry));

        entry.id = "ffffffff-ffff-7fff-8fff-ffffffffffff".to_string();
        assert!(!cursor.includes(&entry));
    }

    #[test]
    fn the_latest_cursor_handles_identical_timestamps() {
        let mut first = entry(500);
        first.id = "00000000-0000-7000-8000-000000000001".into();
        let mut second = entry(500);
        second.id = "00000000-0000-7000-8000-000000000002".into();

        let latest = Cursor::latest(&[first.clone(), second.clone()]).unwrap();
        assert_eq!(latest.id, second.id);
        assert!(latest.includes(&first));
        assert!(latest.includes(&second));
        assert_eq!(Cursor::latest(&[]), None);
    }

    #[test]
    fn a_batch_reports_what_it_applied() {
        let local = entry(100);
        let mut report = MergeReport::default();
        report.record(merge(Some(&local), &entry(200)).is_some());
        report.record(merge(Some(&local), &entry(50)).is_some());
        assert_eq!(report.applied, 1);
        assert_eq!(report.skipped, 1);
    }
}
