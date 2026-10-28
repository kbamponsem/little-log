//! The projection: the SQLite rows, rebuilt from the event log.
//!
//! The log is the source of truth; every row in `entries` is derived from it.
//! Nothing else may write those tables — two writers with two different conflict
//! rules is how a log and its view drift apart, and the drift is invisible until
//! someone notices a number that disagrees with the events that produced it.
//!
//! [`resolve`] is a pure function of an entry's *set* of events, which is the
//! property that matters: gossip and anti-entropy both deliver out of order, so
//! a rebuild and an incremental apply have to agree regardless of arrival order.

use std::collections::BTreeMap;

use crate::clock::Hlc;
use crate::crypto::GroupKey;
use crate::error::Result;
use crate::event::Event;
use crate::model::{Entry, EventKind, Millis};
use crate::payload::{BabyPayload, EntryPayload};
use crate::repo::Repo;

/// Fold every event about one entry into the row it describes.
///
/// `None` means there is nothing to show and therefore no row to write: either
/// there is no payload-bearing event at all, or none we can open — an event
/// sealed with a group key this device does not hold.
pub fn resolve(events: &[Event], group: &GroupKey) -> Result<Option<Entry>> {
    // Two registers, each a max over the events: the newest write, and the
    // newest delete. Comparing whole HLCs is what makes both order-independent.
    let mut newest_write: Option<(&Event, EntryPayload)> = None;
    let mut newest_delete: Option<&Hlc> = None;
    let mut earliest_write = Millis::MAX;
    let mut latest_change = Millis::MIN;

    for event in events {
        match event.kind {
            EventKind::Delete => {
                if newest_delete.is_none_or(|seen| &event.hlc > seen) {
                    newest_delete = Some(&event.hlc);
                }
                latest_change = latest_change.max(event.hlc.wall_ms);
            }
            // create, edit, sleep_start and sleep_stop all carry a whole state.
            // A sleep is a session whose start and stop are two writes to the
            // same register, which is why a second concurrent stop cannot
            // produce a second wake-up: one of them simply loses.
            //
            // A profile is about a baby, not an entry, so it is not entry
            // state even when its subject id happens to collide.
            EventKind::Profile => continue,
            _ => {
                // A payload we cannot open is one we do not hold the key for, or
                // one that did not survive the trip. Skip it: the rest of the
                // entry is still perfectly readable.
                let Ok(plain) = event.open(group) else {
                    continue;
                };
                let Ok(payload) = EntryPayload::from_bytes(&plain) else {
                    continue;
                };

                if newest_write
                    .as_ref()
                    .is_none_or(|(best, _)| event.hlc > best.hlc)
                {
                    newest_write = Some((event, payload));
                }
                earliest_write = earliest_write.min(event.hlc.wall_ms);
                latest_change = latest_change.max(event.hlc.wall_ms);
            }
        }
    }

    let Some((winner, payload)) = newest_write else {
        return Ok(None);
    };

    // A delete hides the entry only when it is newer than the write it hides. A
    // write that is newer than every delete is a real reversal and wins — which
    // is what makes undeleting possible at all.
    let deleted = newest_delete.is_some_and(|deleted_at| deleted_at > &winner.hlc);

    Ok(Some(Entry {
        id: winner.entry_id.clone(),
        baby_id: Some(winner.topic.clone()),
        kind: payload.kind,
        occurred_at: payload.occurred_at,
        ended_at: payload.ended_at,
        feed_kind: payload.feed_kind,
        amount: payload.amount,
        amount_unit: payload.amount_unit,
        diaper_type: payload.diaper_type,
        pee_shade: payload.pee_shade,
        pee_amount: payload.pee_amount,
        poop_color: payload.poop_color,
        consistency: payload.consistency,
        poop_amount: payload.poop_amount,
        sleep_location: payload.sleep_location,
        wake_mood: payload.wake_mood,
        mood: payload.mood,
        note: payload.note,
        photo: payload.photo,
        caregiver_id: Some(winner.author.clone()),
        created_at: if earliest_write == Millis::MAX {
            winner.hlc.wall_ms
        } else {
            earliest_write
        },
        updated_at: latest_change,
        deleted,
    }))
}

/// A baby as the log describes it.
///
/// [`Baby`] is the profile the UI sees; the projected form adds the bookkeeping
/// a row needs (timestamps, tombstone). Appearance is not here at all: it is a
/// device setting, so it is not something a profile event can decide.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedBaby {
    pub id: String,
    pub name: String,
    pub born: String,
    pub photo: Option<String>,
    pub created_at: Millis,
    pub updated_at: Millis,
    pub deleted: bool,
}

/// Fold a baby's profile events into the profile they describe.
///
/// The same two registers as [`resolve`]: the newest profile write wins, and a
/// delete hides it only when it is newer than that write. Arrival order cannot
/// matter, which is what makes replicas agree without a server.
pub fn resolve_baby(events: &[Event], group: &GroupKey) -> Result<Option<ResolvedBaby>> {
    let mut newest_write: Option<(&Event, BabyPayload)> = None;
    let mut newest_delete: Option<&Hlc> = None;
    let mut earliest_write = Millis::MAX;
    let mut latest_change = Millis::MIN;

    for event in events {
        match event.kind {
            EventKind::Delete => {
                if newest_delete.is_none_or(|seen| &event.hlc > seen) {
                    newest_delete = Some(&event.hlc);
                }
                latest_change = latest_change.max(event.hlc.wall_ms);
            }
            EventKind::Profile => {
                let Ok(plain) = event.open(group) else {
                    continue;
                };
                let Ok(payload) = BabyPayload::from_bytes(&plain) else {
                    continue;
                };

                if newest_write
                    .as_ref()
                    .is_none_or(|(best, _)| event.hlc > best.hlc)
                {
                    newest_write = Some((event, payload));
                }
                earliest_write = earliest_write.min(event.hlc.wall_ms);
                latest_change = latest_change.max(event.hlc.wall_ms);
            }
            // An entry event is about an entry, never a profile.
            _ => continue,
        }
    }

    let Some((winner, payload)) = newest_write else {
        return Ok(None);
    };

    Ok(Some(ResolvedBaby {
        id: winner.entry_id.clone(),
        name: payload.name,
        born: payload.born,
        photo: payload.photo,
        created_at: if earliest_write == Millis::MAX {
            winner.hlc.wall_ms
        } else {
            earliest_write
        },
        updated_at: latest_change,
        deleted: newest_delete.is_some_and(|deleted_at| deleted_at > &winner.hlc),
    }))
}

impl Repo {
    /// Fold one event into the row it describes, whichever kind of row that is.
    ///
    /// The receiving side needs this: an event that has just arrived in the log
    /// is not visible anywhere until it is folded, and a profile event describes
    /// a *child* rather than an entry, so the caller must not have to know which
    /// it is holding.
    pub fn project(&self, event: &Event, group: &GroupKey) -> Result<()> {
        match event.kind {
            EventKind::Profile => self.project_baby(&event.entry_id, group),
            EventKind::Delete => self.project_entry(&event.entry_id, group),
            _ => self.project_entry(&event.entry_id, group),
        }
    }

    /// Fold one entry's events back into its row.
    ///
    /// Call this after inserting anything that touches an entry, however many
    /// events arrive and in whatever order — re-resolving from the log is what
    /// keeps the row a function of the events rather than of their history.
    pub fn project_entry(&self, entry_id: &str, group: &GroupKey) -> Result<()> {
        let events = self.log().for_entry(entry_id)?;
        if let Some(entry) = resolve(&events, group)? {
            self.force_entry(&entry)?;
        }
        Ok(())
    }

    /// Fold one baby's profile events back into its row.
    ///
    /// The row lives in `babies` alongside the entries, so a profile is
    /// projected exactly like an entry: re-read the subject's events, fold, and
    /// overwrite what the fold decides.
    pub fn project_baby(&self, baby_id: &str, group: &GroupKey) -> Result<()> {
        let events = self.log().for_entry(baby_id)?;
        if let Some(baby) = resolve_baby(&events, group)? {
            self.force_baby(&baby)?;
        }
        Ok(())
    }

    /// Write a projected baby row.
    ///
    /// Only what a profile can decide is written: name, birthday and picture.
    /// The theme is not here at all — it lives in `device_settings`, so folding
    /// a profile that arrived from another phone cannot change how this one
    /// looks.
    pub fn force_baby(&self, baby: &ResolvedBaby) -> Result<()> {
        self.connection().execute(
            "INSERT INTO babies (id, name, born, photo, created_at, updated_at, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                born = excluded.born,
                photo = excluded.photo,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                deleted = excluded.deleted",
            rusqlite::params![
                baby.id,
                baby.name,
                baby.born,
                baby.photo,
                baby.created_at,
                baby.updated_at,
                i64::from(baby.deleted),
            ],
        )?;
        Ok(())
    }

    /// Rebuild every row from the log, and return how many rows that leaves.
    ///
    /// Equal by construction to applying events one at a time, because
    /// [`resolve`] and [`resolve_baby`] fold a subject's whole event set either
    /// way. A test holds that claim to account rather than leaving it as a
    /// comment.
    pub fn rebuild_projection(&self, group: &GroupKey) -> Result<usize> {
        // Profile events are keyed by the baby's id, entry events by the
        // entry's, so they are grouped separately and folded by their own rule.
        let mut by_entry: BTreeMap<String, Vec<Event>> = BTreeMap::new();
        let mut by_baby: BTreeMap<String, Vec<Event>> = BTreeMap::new();
        for event in self.log().all()? {
            if event.kind == EventKind::Profile {
                by_baby
                    .entry(event.entry_id.clone())
                    .or_default()
                    .push(event);
            } else {
                by_entry
                    .entry(event.entry_id.clone())
                    .or_default()
                    .push(event);
            }
        }

        let mut written = 0;
        for (_, events) in by_entry {
            if let Some(entry) = resolve(&events, group)? {
                self.force_entry(&entry)?;
                written += 1;
            }
        }
        for (_, events) in by_baby {
            if let Some(baby) = resolve_baby(&events, group)? {
                self.force_baby(&baby)?;
                written += 1;
            }
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::DeviceKey;
    use crate::model::{AmountUnit, EntryKind, FeedKind, Theme};

    /// A device with its own database: a key to sign with, a group key to seal
    /// with, and a log to fold.
    struct Peer {
        key: DeviceKey,
        group: GroupKey,
        repo: Repo,
    }

    impl Peer {
        fn new() -> Self {
            Self::with_group(&GroupKey::generate().unwrap())
        }

        fn with_group(group: &GroupKey) -> Self {
            Self {
                key: DeviceKey::generate().unwrap(),
                group: GroupKey::from_bytes(group.to_bytes()),
                repo: Repo::open_in_memory().unwrap(),
            }
        }

        /// Author an event without storing it.
        fn author(&self, entry_id: &str, kind: EventKind, at: i64, amount: i64) -> Event {
            self.author_for("baby-1", entry_id, kind, at, amount)
        }

        /// Author an entry event for a chosen baby (the event's topic).
        fn author_for(
            &self,
            baby_id: &str,
            entry_id: &str,
            kind: EventKind,
            at: i64,
            amount: i64,
        ) -> Event {
            let mut payload = EntryPayload::of(&Entry::new(EntryKind::Feed, at));
            payload.kind = EntryKind::Feed;
            payload.feed_kind = Some(FeedKind::Bottle);
            payload.amount = Some(amount);
            payload.amount_unit = Some(AmountUnit::Milliliters);

            Event::seal(
                &self.key,
                &self.group,
                baby_id,
                entry_id,
                kind,
                Hlc::new(at, 0, self.key.id()),
                &payload.to_bytes().unwrap(),
            )
            .unwrap()
        }

        /// Author a profile event for `baby_id`; its subject id is the baby's.
        fn profile(&self, baby_id: &str, kind: EventKind, at: i64, name: &str) -> Event {
            let payload = BabyPayload {
                name: name.to_string(),
                born: "2026-02-14".to_string(),
                photo: None,
                photo_data: None,
            };
            Event::seal(
                &self.key,
                &self.group,
                baby_id,
                baby_id,
                kind,
                Hlc::new(at, 0, self.key.id()),
                &payload.to_bytes().unwrap(),
            )
            .unwrap()
        }

        /// Store a profile event and fold it into the baby row.
        fn apply_baby(&self, event: &Event) -> bool {
            let inserted = self.repo.log().insert(event).unwrap();
            self.repo
                .project_baby(&event.entry_id, &self.group)
                .unwrap();
            inserted
        }

        fn delete(&self, entry_id: &str, at: i64) -> Event {
            self.delete_in("baby-1", entry_id, at)
        }

        /// Tombstone a subject on a chosen baby's topic.
        fn delete_in(&self, baby_id: &str, entry_id: &str, at: i64) -> Event {
            Event::seal(
                &self.key,
                &self.group,
                baby_id,
                entry_id,
                EventKind::Delete,
                Hlc::new(at, 0, self.key.id()),
                b"",
            )
            .unwrap()
        }

        /// Store an event and bring the projection up to date with it.
        fn apply(&self, event: &Event) -> bool {
            let inserted = self.repo.log().insert(event).unwrap();
            self.repo
                .project_entry(&event.entry_id, &self.group)
                .unwrap();
            inserted
        }

        fn row(&self, entry_id: &str) -> Option<Entry> {
            self.repo
                .connection()
                .query_row(
                    "SELECT id, baby_id, kind, occurred_at, ended_at, feed_kind, amount, amount_unit,
                            diaper_type, pee_shade, pee_amount, poop_color, consistency, poop_amount,
                            sleep_location, wake_mood, note, photo, caregiver_id, created_at,
                            updated_at, deleted, mood
                       FROM entries WHERE id = ?1",
                    rusqlite::params![entry_id],
                    |row| {
                        Ok(Entry {
                            id: row.get(0)?,
                            baby_id: row.get(1)?,
                            kind: row.get::<_, String>(2)?.parse().unwrap(),
                            occurred_at: row.get(3)?,
                            ended_at: row.get(4)?,
                            feed_kind: row.get::<_, Option<String>>(5)?.map(|v| v.parse().unwrap()),
                            amount: row.get(6)?,
                            amount_unit: row
                                .get::<_, Option<String>>(7)?
                                .map(|v| v.parse().unwrap()),
                            diaper_type: row.get::<_, Option<String>>(8)?.map(|v| v.parse().unwrap()),
                            pee_shade: row.get::<_, Option<String>>(9)?.map(|v| v.parse().unwrap()),
                            pee_amount: row.get::<_, Option<String>>(10)?.map(|v| v.parse().unwrap()),
                            poop_color: row.get::<_, Option<String>>(11)?.map(|v| v.parse().unwrap()),
                            consistency: row
                                .get::<_, Option<String>>(12)?
                                .map(|v| v.parse().unwrap()),
                            poop_amount: row
                                .get::<_, Option<String>>(13)?
                                .map(|v| v.parse().unwrap()),
                            sleep_location: row
                                .get::<_, Option<String>>(14)?
                                .map(|v| v.parse().unwrap()),
                            wake_mood: row.get::<_, Option<String>>(15)?.map(|v| v.parse().unwrap()),
                            note: row.get(16)?,
                            photo: row.get(17)?,
                            caregiver_id: row.get(18)?,
                            created_at: row.get(19)?,
                            updated_at: row.get(20)?,
                            deleted: row.get::<_, i64>(21)? != 0,
                            mood: row.get::<_, Option<String>>(22)?.map(|v| v.parse().unwrap()),
                        })
                    },
                )
                .ok()
        }

        fn rows(&self) -> i64 {
            self.repo
                .connection()
                .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
                .unwrap()
        }
    }

    #[test]
    fn the_newest_write_wins() {
        let peer = Peer::new();
        let create = peer.author("e1", EventKind::Create, 1_000, 100);
        let edit = peer.author("e1", EventKind::Edit, 2_000, 150);

        let resolved = resolve(&[create, edit], &peer.group).unwrap().unwrap();

        assert_eq!(resolved.amount, Some(150));
        assert_eq!(resolved.id, "e1");
        assert_eq!(resolved.baby_id.as_deref(), Some("baby-1"));
        assert!(!resolved.deleted);
    }

    #[test]
    fn arrival_order_cannot_change_the_answer() {
        // The property the whole design rests on: anti-entropy delivers whatever
        // it likes, in whatever order, and every device still agrees.
        let peer = Peer::new();
        let create = peer.author("e1", EventKind::Create, 1_000, 100);
        let edit = peer.author("e1", EventKind::Edit, 2_000, 150);
        let later = peer.author("e1", EventKind::Edit, 3_000, 200);

        let forwards =
            resolve(&[create.clone(), edit.clone(), later.clone()], &peer.group).unwrap();
        let backwards =
            resolve(&[later.clone(), edit.clone(), create.clone()], &peer.group).unwrap();
        let shuffled = resolve(&[edit, later, create], &peer.group).unwrap();

        assert_eq!(forwards, backwards);
        assert_eq!(forwards, shuffled);
        assert_eq!(forwards.unwrap().amount, Some(200));
    }

    #[test]
    fn a_delete_newer_than_the_write_hides_the_entry() {
        let peer = Peer::new();
        let create = peer.author("e1", EventKind::Create, 1_000, 100);
        let delete = peer.delete("e1", 2_000);

        let resolved = resolve(&[create, delete], &peer.group).unwrap().unwrap();

        assert!(resolved.deleted);
        assert_eq!(resolved.updated_at, 2_000, "the delete is the last change");
    }

    #[test]
    fn an_edit_newer_than_the_delete_brings_the_entry_back() {
        let peer = Peer::new();
        let create = peer.author("e1", EventKind::Create, 1_000, 100);
        let delete = peer.delete("e1", 2_000);
        let restore = peer.author("e1", EventKind::Edit, 3_000, 120);

        let resolved = resolve(&[create, delete, restore], &peer.group)
            .unwrap()
            .unwrap();

        assert!(!resolved.deleted);
        assert_eq!(resolved.amount, Some(120));
    }

    #[test]
    fn a_late_edit_of_an_old_copy_does_not_undo_a_delete() {
        // The case the old cursor-based merge got wrong: a device that was
        // offline edits its stale copy, and the edit arrives after the delete.
        let peer = Peer::new();
        let create = peer.author("e1", EventKind::Create, 1_000, 100);
        let stale_edit = peer.author("e1", EventKind::Edit, 1_500, 110);
        let delete = peer.delete("e1", 2_000);

        let resolved = resolve(&[create, stale_edit, delete], &peer.group)
            .unwrap()
            .unwrap();

        assert!(resolved.deleted, "the newer delete still wins");
    }

    #[test]
    fn an_event_sealed_with_a_key_we_do_not_hold_leaves_nothing_to_show() {
        let theirs = Peer::new();
        let ours = Peer::with_group(&GroupKey::generate().unwrap());
        let create = theirs.author("e1", EventKind::Create, 1_000, 100);

        assert!(resolve(&[create], &ours.group).unwrap().is_none());
    }

    #[test]
    fn a_payload_that_does_not_decode_does_not_break_the_fold() {
        let peer = Peer::new();
        let good = peer.author("e1", EventKind::Create, 1_000, 100);

        // Sealed properly with the right key — but the bytes inside are not an
        // entry. Skipping it must not cost us the readable write.
        let rubbish = Event::seal(
            &peer.key,
            &peer.group,
            "baby-1",
            "e1",
            EventKind::Edit,
            Hlc::new(2_000, 0, peer.key.id()),
            b"not json at all",
        )
        .unwrap();

        let resolved = resolve(&[good, rubbish], &peer.group).unwrap().unwrap();

        assert_eq!(resolved.amount, Some(100), "the readable write still wins");
    }

    #[test]
    fn timestamps_bracket_the_events_that_made_the_entry() {
        let peer = Peer::new();
        let create = peer.author("e1", EventKind::Create, 1_000, 100);
        let edit = peer.author("e1", EventKind::Edit, 5_000, 150);

        let resolved = resolve(&[edit, create], &peer.group).unwrap().unwrap();

        assert_eq!(resolved.created_at, 1_000, "earliest write");
        assert_eq!(resolved.updated_at, 5_000, "latest change");
    }

    #[test]
    fn the_author_of_the_winning_write_is_recorded() {
        let phone = Peer::with_group(&GroupKey::generate().unwrap());
        let tablet = Peer::with_group(&phone.group);
        let from_phone = phone.author("e1", EventKind::Create, 1_000, 100);
        let from_tablet = tablet.author("e1", EventKind::Edit, 2_000, 150);

        let resolved = resolve(&[from_phone, from_tablet], &phone.group)
            .unwrap()
            .unwrap();

        assert_eq!(
            resolved.caregiver_id.as_deref(),
            Some(tablet.key.id().as_str())
        );
    }

    #[test]
    fn applying_events_out_of_order_matches_a_clean_rebuild() {
        // Two databases, the same events: one folds them as they arrive (worst
        // case, newest first), the other rebuilds from the full log afterwards.
        let author = Peer::new();
        let events = vec![
            author.author("e1", EventKind::Create, 1_000, 100),
            author.author("e1", EventKind::Edit, 4_000, 140),
            author.delete("e1", 5_000),
            author.author("e1", EventKind::Edit, 6_000, 160),
            author.author("e2", EventKind::Create, 2_000, 90),
            author.author("e2", EventKind::Create, 1_500, 80),
            author.delete("e2", 3_000),
            author.author("e3", EventKind::Create, 7_000, 70),
        ];

        let incremental = Peer::with_group(&author.group);
        for event in events.iter().rev() {
            incremental.apply(event);
        }

        let rebuilt = Peer::with_group(&author.group);
        for event in &events {
            rebuilt.repo.log().insert(event).unwrap();
        }
        let written = rebuilt.repo.rebuild_projection(&rebuilt.group).unwrap();

        assert_eq!(written, 3, "three entries in the log");
        assert_eq!(incremental.rows(), rebuilt.rows(), "same number of rows");
        for entry_id in ["e1", "e2", "e3"] {
            assert_eq!(
                incremental.row(entry_id),
                rebuilt.row(entry_id),
                "{entry_id} must project the same either way"
            );
        }

        // And the visible ones are what the UI would show.
        let live = rebuilt.repo.entries().unwrap();
        assert_eq!(live.len(), 2, "e2 is tombstoned");
        assert!(live.iter().all(|entry| entry.id != "e2"));
    }

    #[test]
    fn a_tombstoned_entry_stays_out_of_the_visible_list() {
        let peer = Peer::new();
        peer.apply(&peer.author("e1", EventKind::Create, 1_000, 100));
        assert_eq!(peer.repo.entries().unwrap().len(), 1);

        peer.apply(&peer.delete("e1", 2_000));

        assert!(peer.repo.entries().unwrap().is_empty());
        assert_eq!(peer.repo.count_entries().unwrap(), 0);
        assert_eq!(peer.rows(), 1, "the row survives as a tombstone");
        assert!(peer.row("e1").unwrap().deleted);
    }

    #[test]
    fn re_applying_the_same_event_changes_nothing() {
        let peer = Peer::new();
        let create = peer.author("e1", EventKind::Create, 1_000, 100);
        assert!(peer.apply(&create));

        assert!(!peer.apply(&create), "the log already had it");
        assert_eq!(peer.rows(), 1);
        assert_eq!(peer.row("e1").unwrap().amount, Some(100));
    }

    /* ------------------------------------------------------------- babies */

    #[test]
    fn a_profile_folds_into_a_baby_row() {
        let peer = Peer::new();

        peer.apply_baby(&peer.profile("b1", EventKind::Profile, 1_000, "Ada"));

        let babies = peer.repo.babies().unwrap();
        assert_eq!(babies.len(), 1);
        assert_eq!(babies[0].id.as_deref(), Some("b1"));
        assert_eq!(babies[0].name, "Ada");
        assert_eq!(babies[0].born, "2026-02-14");
    }

    #[test]
    fn the_newest_profile_write_names_the_baby() {
        // Same rule as entries: the newest write wins, whatever the order it
        // arrived in. Two phones renaming a baby must not disagree.
        let peer = Peer::new();
        let first = peer.profile("b1", EventKind::Profile, 1_000, "Ada");
        let second = peer.profile("b1", EventKind::Profile, 2_000, "Ada Lovelace");

        let forwards = resolve_baby(&[first.clone(), second.clone()], &peer.group)
            .unwrap()
            .unwrap();
        let backwards = resolve_baby(&[second, first], &peer.group)
            .unwrap()
            .unwrap();

        assert_eq!(forwards, backwards);
        assert_eq!(forwards.name, "Ada Lovelace");
        assert_eq!(forwards.created_at, 1_000);
        assert_eq!(forwards.updated_at, 2_000);
    }

    #[test]
    fn a_profile_tombstone_hides_the_baby_but_keeps_its_entries() {
        let peer = Peer::new();
        peer.apply_baby(&peer.profile("b1", EventKind::Profile, 1_000, "Ada"));
        peer.apply(&peer.author_for("b1", "e1", EventKind::Create, 1_500, 100));

        peer.apply_baby(&peer.delete_in("b1", "b1", 2_000));

        assert!(peer.repo.babies().unwrap().is_empty());
        assert!(peer.repo.baby_by_id("b1").unwrap().is_none());
        assert_eq!(
            peer.repo.entries_for("b1").unwrap().len(),
            1,
            "the log is shared; tombstoning a profile is not a purge"
        );
    }

    #[test]
    fn two_babies_keep_their_entries_apart() {
        let peer = Peer::new();
        peer.apply_baby(&peer.profile("b1", EventKind::Profile, 1_000, "Ada"));
        peer.apply_baby(&peer.profile("b2", EventKind::Profile, 2_000, "Bo"));
        peer.apply(&peer.author_for("b1", "e1", EventKind::Create, 3_000, 100));
        peer.apply(&peer.author_for("b2", "e2", EventKind::Create, 4_000, 120));

        assert_eq!(peer.repo.babies().unwrap().len(), 2);
        assert_eq!(peer.repo.entries_for("b1").unwrap().len(), 1);
        assert_eq!(peer.repo.entries_for("b2").unwrap().len(), 1);
        assert_eq!(peer.repo.entries().unwrap().len(), 2, "both, unfiltered");
    }

    #[test]
    fn a_running_sleep_belongs_to_one_baby() {
        let peer = Peer::new();
        peer.apply_baby(&peer.profile("b1", EventKind::Profile, 1_000, "Ada"));
        peer.apply_baby(&peer.profile("b2", EventKind::Profile, 1_000, "Bo"));

        let mut asleep = Entry::new(EntryKind::Sleep, 2_000);
        asleep.id = "s1".into();
        asleep.baby_id = Some("b1".into());
        let event = Event::seal(
            &peer.key,
            &peer.group,
            "b1",
            "s1",
            EventKind::SleepStart,
            Hlc::new(2_000, 0, peer.key.id()),
            &EntryPayload::of(&asleep).to_bytes().unwrap(),
        )
        .unwrap();
        peer.apply(&event);

        assert!(peer.repo.running_sleep_for("b1").unwrap().is_some());
        assert!(
            peer.repo.running_sleep_for("b2").unwrap().is_none(),
            "Bo is not the one asleep"
        );
    }

    #[test]
    fn a_projected_profile_leaves_this_devices_theme_alone() {
        // Appearance lives in `device_settings`, which no profile event touches.
        let peer = Peer::new();
        peer.repo.save_device_theme(Theme::Dark).unwrap();
        let baby = peer.repo.ensure_baby("Ada").unwrap();
        let id = baby.id.clone().unwrap();

        peer.apply_baby(&peer.profile(&id, EventKind::Profile, 2_000, "Ada Lovelace"));

        assert_eq!(
            peer.repo.baby_by_id(&id).unwrap().unwrap().name,
            "Ada Lovelace"
        );
        assert_eq!(peer.repo.device_theme().unwrap(), Theme::Dark);
    }

    #[test]
    fn one_event_folds_into_the_row_it_describes() {
        // What a peer's event needs before it is visible: the profile creates a
        // child, an entry write creates the row.
        let peer = Peer::new();
        let profile = peer.profile("b1", EventKind::Profile, 1_000, "Ada");
        let entry = peer.author_for("b1", "e1", EventKind::Create, 2_000, 100);

        for event in [&profile, &entry] {
            peer.repo.log().insert(event).unwrap();
            peer.repo.project(event, &peer.group).unwrap();
        }

        assert_eq!(peer.repo.babies().unwrap().len(), 1, "the child arrived");
        assert_eq!(peer.repo.babies().unwrap()[0].name, "Ada");
        assert_eq!(
            peer.repo.entries().unwrap().len(),
            1,
            "and so did the entry"
        );
    }

    #[test]
    fn a_clean_rebuild_restores_every_baby_and_entry() {
        let author = Peer::new();
        let events = [
            author.profile("b1", EventKind::Profile, 1_000, "Ada"),
            author.profile("b2", EventKind::Profile, 1_100, "Bo"),
            author.author("e1", EventKind::Create, 2_000, 100),
            author.author("e2", EventKind::Create, 2_100, 120),
        ];

        let rebuilt = Peer::with_group(&author.group);
        for event in events.iter().rev() {
            rebuilt.repo.log().insert(event).unwrap();
        }
        let written = rebuilt.repo.rebuild_projection(&rebuilt.group).unwrap();

        assert_eq!(written, 4, "two babies and two entries");
        let names: Vec<String> = rebuilt
            .repo
            .babies()
            .unwrap()
            .into_iter()
            .map(|baby| baby.name)
            .collect();
        assert_eq!(names, vec!["Ada", "Bo"], "oldest first");
        assert_eq!(rebuilt.repo.entries().unwrap().len(), 2);
    }
}
