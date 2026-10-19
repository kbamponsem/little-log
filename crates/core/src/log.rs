//! The append-only event log.
//!
//! The log is the source of truth; every other table is a projection that can be
//! rebuilt from it. Two rules make replication simple:
//!
//! - **Append only.** Events are never updated or deleted, so a peer can always
//!   be caught up by sending whatever it does not already have.
//! - **Set union.** Re-inserting an event is a no-op, so gossip re-sends and
//!   anti-entropy retries cost nothing and cannot corrupt anything.
//!
//! Signatures are checked on the way in, not by the caller: a log that accepted
//! an unverifiable event would poison every device that later synced from it.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::clock::Hlc;
use crate::error::{CoreError, Result};
use crate::event::Event;
use crate::repo::{conversion_error, parse_req};

// `entry_id` is last because V4 adds it with `ALTER TABLE`, which appends.
const COLUMNS: &str =
    "id, topic, author, wall_ms, counter, node, kind, nonce, payload, signature, entry_id";

/// A view of one database's event log.
pub struct EventLog<'a> {
    conn: &'a Connection,
}

impl<'a> EventLog<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Add an event, verifying its signature first.
    ///
    /// Returns `false` when the event was already present — the caller can treat
    /// that as success, because the log is a set.
    pub fn insert(&self, event: &Event) -> Result<bool> {
        event.verify()?;
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO events
                (id, entry_id, topic, author, wall_ms, counter, node, kind, nonce, payload, signature)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                event.id,
                event.entry_id,
                event.topic,
                event.author,
                event.hlc.wall_ms,
                event.hlc.counter,
                event.hlc.node,
                event.kind.as_str(),
                &event.nonce[..],
                event.payload,
                &event.signature[..],
            ],
        )?;
        Ok(inserted > 0)
    }

    pub fn get(&self, id: &str) -> Result<Option<Event>> {
        let sql = format!("SELECT {COLUMNS} FROM events WHERE id = ?1");
        self.conn
            .query_row(&sql, params![id], event_from_row)
            .optional()
            .map_err(CoreError::from)
    }

    /// Every event, oldest first — the order the projection expects to fold.
    pub fn all(&self) -> Result<Vec<Event>> {
        let sql = format!("SELECT {COLUMNS} FROM events ORDER BY wall_ms, counter, node");
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([], event_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    pub fn for_topic(&self, topic: &str) -> Result<Vec<Event>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM events WHERE topic = ?1 ORDER BY wall_ms, counter, node"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![topic], event_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    /// Every event about one entry, oldest first — what the projection folds.
    pub fn for_entry(&self, entry_id: &str) -> Result<Vec<Event>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM events WHERE entry_id = ?1 ORDER BY wall_ms, counter, node"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![entry_id], event_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    pub fn count(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .map_err(CoreError::from)
    }

    /// The newest timestamp from each device: the version vector a peer sends so
    /// the other side knows what it is missing.
    ///
    /// Keyed by the HLC's node, which is the authoring device — so two events
    /// from one device compare on `(wall_ms, counter)` alone, exactly as merge
    /// order requires.
    pub fn heads(&self) -> Result<Vec<Hlc>> {
        self.heads_for(None)
    }

    /// The same vector restricted to one topic.
    ///
    /// This is what a per-baby exchange must compare: a device's newest event
    /// *anywhere* says nothing about the topic a peer is behind on, and using
    /// the global vector would silently skip a topic's older events.
    pub fn heads_for_topic(&self, topic: &str) -> Result<Vec<Hlc>> {
        self.heads_for(Some(topic))
    }

    fn heads_for(&self, topic: Option<&str>) -> Result<Vec<Hlc>> {
        let filter = if topic.is_some() {
            " WHERE topic = ?1"
        } else {
            ""
        };
        let sql = format!(
            "SELECT wall_ms, counter, node FROM events{filter}
             ORDER BY wall_ms DESC, counter DESC, node DESC"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = match topic {
            Some(topic) => statement.query_map(params![topic], hlc_from_row)?,
            None => statement.query_map([], hlc_from_row)?,
        };

        // One head per node: the first (newest) row wins, the rest are noise.
        let mut seen: Vec<Hlc> = Vec::new();
        for head in rows {
            let head = head?;
            if !seen.iter().any(|known| known.node == head.node) {
                seen.push(head);
            }
        }
        Ok(seen)
    }

    /// Everything this log has that a peer holding `their_heads` does not.
    ///
    /// This is the whole of anti-entropy: send the reply, or merge it straight
    /// into the local log — both are safe, because insertion is set union.
    pub fn missing(&self, their_heads: &[Hlc]) -> Result<Vec<Event>> {
        self.missing_for(None, their_heads)
    }

    /// The same gap, restricted to one topic.
    pub fn missing_for_topic(&self, topic: &str, their_heads: &[Hlc]) -> Result<Vec<Event>> {
        self.missing_for(Some(topic), their_heads)
    }

    fn missing_for(&self, topic: Option<&str>, their_heads: &[Hlc]) -> Result<Vec<Event>> {
        let known: HashMap<&str, &Hlc> = their_heads
            .iter()
            .map(|head| (head.node.as_str(), head))
            .collect();

        let events = match topic {
            Some(topic) => self.for_topic(topic)?,
            None => self.all()?,
        };

        Ok(events
            .into_iter()
            .filter(|event| match known.get(event.hlc.node.as_str()) {
                // A device they have never heard from: send everything.
                None => true,
                Some(head) => event.hlc > **head,
            })
            .collect())
    }
}

fn hlc_from_row(row: &Row<'_>) -> rusqlite::Result<Hlc> {
    Ok(Hlc::new(
        row.get::<_, i64>(0)?,
        row.get::<_, u32>(1)?,
        row.get::<_, String>(2)?,
    ))
}

fn event_from_row(row: &Row<'_>) -> rusqlite::Result<Event> {
    let kind: String = row.get(6)?;
    Ok(Event {
        id: row.get(0)?,
        topic: row.get(1)?,
        author: row.get(2)?,
        hlc: Hlc::new(row.get(3)?, row.get(4)?, row.get::<_, String>(5)?),
        kind: parse_req(kind, 6)?,
        nonce: fixed(row.get(7)?, 7)?,
        payload: row.get(8)?,
        signature: fixed(row.get(9)?, 9)?,
        entry_id: row.get(10)?,
    })
}

/// A fixed-size BLOB column. Any other length means the row is not ours.
fn fixed<const N: usize>(bytes: Vec<u8>, column: usize) -> rusqlite::Result<[u8; N]> {
    bytes
        .try_into()
        .map_err(|_| conversion_error(column, CoreError::data("blob column has the wrong length")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{DeviceKey, GroupKey};
    use crate::model::EventKind;
    use crate::repo::Repo;

    struct Peer {
        key: DeviceKey,
        group: GroupKey,
        repo: Repo,
    }

    impl Peer {
        /// A peer with its own database, so the log can be tested as a set.
        fn new(group: &GroupKey) -> Self {
            Self {
                key: DeviceKey::generate().unwrap(),
                group: GroupKey::from_bytes(group.to_bytes()),
                repo: Repo::open_in_memory().unwrap(),
            }
        }

        fn log(&self) -> EventLog<'_> {
            self.repo.log()
        }

        /// Author an event without storing it — for testing what happens when
        /// we *try* to store something.
        fn make_event(&self, at: i64, kind: EventKind, body: &[u8]) -> Event {
            Event::seal(
                &self.key,
                &self.group,
                "baby-1",
                "entry-1",
                kind,
                Hlc::new(at, 0, self.key.id()),
                body,
            )
            .unwrap()
        }

        /// Author and store an event; `at` keeps the clock deterministic.
        fn log_event(&self, at: i64, kind: EventKind, body: &[u8]) -> Event {
            self.record(self.make_event(at, kind, body))
        }

        fn record(&self, event: Event) -> Event {
            assert!(self.log().insert(&event).unwrap(), "should be new");
            event
        }
    }

    #[test]
    fn an_event_round_trips_through_the_log() {
        let group = GroupKey::generate().unwrap();
        let peer = Peer::new(&group);
        let event = peer.log_event(1_000, EventKind::Create, b"{\"amount\":120}");

        let stored = peer.log().get(&event.id).unwrap().unwrap();

        assert_eq!(stored, event);
        assert_eq!(stored.kind, EventKind::Create);
        assert_eq!(stored.hlc, event.hlc);
        assert_eq!(stored.open(&group).unwrap(), b"{\"amount\":120}");
    }

    #[test]
    fn re_inserting_an_event_is_a_noop() {
        let group = GroupKey::generate().unwrap();
        let peer = Peer::new(&group);
        let event = peer.log_event(1_000, EventKind::Create, b"once");

        // The second time round is what makes gossip re-sends free.
        assert!(!peer.log().insert(&event).unwrap());
        assert_eq!(peer.log().count().unwrap(), 1);
        assert_eq!(peer.log().all().unwrap(), vec![event]);
    }

    #[test]
    fn an_unverifiable_event_is_refused_at_the_door() {
        let group = GroupKey::generate().unwrap();
        let peer = Peer::new(&group);

        // Never stored first: this is a forgery arriving over the wire.
        let mut forged = peer.make_event(1_000, EventKind::Create, b"body");
        forged.payload[0] ^= 0x01;

        assert!(peer.log().insert(&forged).is_err());
        assert_eq!(peer.log().count().unwrap(), 0);
    }

    #[test]
    fn events_come_back_in_merge_order() {
        let group = GroupKey::generate().unwrap();
        let peer = Peer::new(&group);

        let third = peer.log_event(3_000, EventKind::Edit, b"c");
        let first = peer.log_event(1_000, EventKind::Create, b"a");
        let second = peer.log_event(2_000, EventKind::Create, b"b");

        let ids: Vec<String> = peer
            .log()
            .all()
            .unwrap()
            .into_iter()
            .map(|event| event.id)
            .collect();

        assert_eq!(ids, vec![first.id, second.id, third.id]);
    }

    #[test]
    fn heads_report_the_newest_event_per_device() {
        let group = GroupKey::generate().unwrap();
        let phone = Peer::new(&group);
        let tablet = Peer::new(&group);

        phone.log_event(1_000, EventKind::Create, b"a");
        let newest = phone.log_event(4_000, EventKind::Edit, b"b");

        // Only the phone's own device is represented, and its head is the
        // newest of the two.
        let heads = phone.log().heads().unwrap();
        assert_eq!(heads, vec![newest.hlc.clone()]);

        // Once a peer's event is merged in, its device is reported too.
        let from_tablet = tablet.log_event(2_000, EventKind::Create, b"c");
        assert!(phone.log().insert(&from_tablet).unwrap());

        let heads = phone.log().heads().unwrap();
        assert_eq!(heads.len(), 2, "one per device");
        assert!(heads.contains(&newest.hlc));
        assert!(heads.contains(&from_tablet.hlc));
    }

    #[test]
    fn a_peer_that_has_nothing_is_missing_everything() {
        let group = GroupKey::generate().unwrap();
        let peer = Peer::new(&group);
        peer.log_event(1_000, EventKind::Create, b"a");
        peer.log_event(2_000, EventKind::Create, b"b");

        assert_eq!(peer.log().missing(&[]).unwrap().len(), 2);
    }

    #[test]
    fn missing_returns_only_what_a_peer_has_not_seen() {
        let group = GroupKey::generate().unwrap();
        let phone = Peer::new(&group);
        let tablet = Peer::new(&group);

        let early = phone.log_event(1_000, EventKind::Create, b"a");
        let late = phone.log_event(5_000, EventKind::Edit, b"b");
        tablet.log_event(2_000, EventKind::Create, b"c");

        // The tablet has seen the early event and its own; it is missing `late`
        // and has never heard from the phone's device at that timestamp.
        let their_heads = vec![early.hlc.clone()];
        let missing = phone.log().missing(&their_heads).unwrap();

        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].id, late.id);
    }

    #[test]
    fn two_peers_converge_by_exchanging_what_is_missing() {
        let group = GroupKey::generate().unwrap();
        let phone = Peer::new(&group);
        let tablet = Peer::new(&group);

        phone.log_event(1_000, EventKind::Create, b"from phone");
        phone.log_event(3_000, EventKind::Edit, b"also phone");
        tablet.log_event(2_000, EventKind::Create, b"from tablet");

        // Phone → tablet, then tablet → phone, each sending only the gap.
        for event in phone.log().missing(&tablet.log().heads().unwrap()).unwrap() {
            assert!(tablet.log().insert(&event).unwrap());
        }
        for event in tablet.log().missing(&phone.log().heads().unwrap()).unwrap() {
            assert!(phone.log().insert(&event).unwrap());
        }

        let phone_ids: Vec<String> = phone
            .log()
            .all()
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        let tablet_ids: Vec<String> = tablet
            .log()
            .all()
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();

        assert_eq!(phone_ids.len(), 3);
        assert_eq!(phone_ids, tablet_ids, "same set, same order");
    }

    #[test]
    fn for_topic_keeps_babies_apart() {
        let group = GroupKey::generate().unwrap();
        let peer = Peer::new(&group);

        peer.log_event(1_000, EventKind::Create, b"ours");
        peer.record(
            Event::seal(
                &peer.key,
                &peer.group,
                "baby-2",
                "entry-2",
                EventKind::Create,
                Hlc::new(2_000, 0, peer.key.id()),
                b"theirs",
            )
            .unwrap(),
        );

        assert_eq!(peer.log().for_topic("baby-1").unwrap().len(), 1);
        assert_eq!(peer.log().for_topic("baby-2").unwrap().len(), 1);
        assert_eq!(peer.log().count().unwrap(), 2);
    }

    #[test]
    fn for_entry_returns_only_the_events_about_one_entry() {
        let group = GroupKey::generate().unwrap();
        let peer = Peer::new(&group);
        let mine = peer.log_event(1_000, EventKind::Create, b"mine");
        let other = peer.record(
            Event::seal(
                &peer.key,
                &peer.group,
                "baby-1",
                "entry-2",
                EventKind::Create,
                Hlc::new(2_000, 0, peer.key.id()),
                b"theirs",
            )
            .unwrap(),
        );

        let for_mine = peer.log().for_entry("entry-1").unwrap();

        assert_eq!(for_mine.len(), 1);
        assert_eq!(for_mine[0].id, mine.id);
        assert_eq!(peer.log().for_entry("entry-2").unwrap()[0].id, other.id);
        assert!(peer.log().for_entry("nothing-here").unwrap().is_empty());
    }
}
