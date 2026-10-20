//! SQLite repository — the single place that knows the schema.
//!
//! Both the app (one file on the device) and the server (one file in the
//! `/data` volume) open a [`Repo`]. Writes go through last-write-wins so the
//! two can never disagree about an entry.

use std::path::Path;
use std::str::FromStr;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::crypto::{DeviceKey, GroupKey};
use crate::error::{CoreError, Result};
use crate::keystore::{self, SqliteKeyStore};
use crate::log::EventLog;
use crate::model::{Baby, Caregiver, Entry, JoinState, Millis, Role, Theme};
use crate::sync::{self, Cursor};
use crate::validate::now_millis;

/// Bumped whenever the schema below changes; see [`Repo::migrate`].
pub const SCHEMA_VERSION: i64 = 9;

const ENTRY_COLUMNS: &str = "id, baby_id, kind, occurred_at, ended_at, feed_kind, amount, \
     amount_unit, diaper_type, pee_shade, pee_amount, poop_color, consistency, poop_amount, \
     sleep_location, wake_mood, note, photo, caregiver_id, created_at, updated_at, deleted";

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS babies (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  born          TEXT NOT NULL DEFAULT '',
  theme         TEXT NOT NULL DEFAULT 'auto',
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  deleted       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS caregivers (
  id            TEXT PRIMARY KEY,
  baby_id       TEXT,
  name          TEXT NOT NULL,
  email         TEXT UNIQUE,
  password_hash TEXT,
  role          TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  deleted       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS entries (
  id            TEXT PRIMARY KEY,
  baby_id       TEXT,
  kind          TEXT NOT NULL,
  occurred_at   INTEGER NOT NULL,
  ended_at      INTEGER,
  feed_kind     TEXT,
  amount        INTEGER,
  amount_unit   TEXT,
  diaper_type   TEXT,
  pee_shade     TEXT,
  pee_amount    TEXT,
  poop_color    TEXT,
  consistency   TEXT,
  poop_amount   TEXT,
  sleep_location TEXT,
  wake_mood     TEXT,
  note          TEXT,
  photo         TEXT,
  caregiver_id  TEXT,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  deleted       INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_entries_updated ON entries(updated_at, id);
CREATE INDEX IF NOT EXISTS idx_entries_occurred ON entries(kind, occurred_at);

-- Server-side sessions and invite links (the app leaves them empty).
CREATE TABLE IF NOT EXISTS sessions (
  token_hash    TEXT PRIMARY KEY,
  caregiver_id  TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  expires_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS invites (
  token_hash    TEXT PRIMARY KEY,
  role          TEXT NOT NULL,
  invited_by    TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  expires_at    INTEGER NOT NULL,
  accepted_by   TEXT
);
"#;

/// The append-only event log — the source of truth the rest of the schema is
/// projected from. Nothing here is ever updated or deleted.
const SCHEMA_V2: &str = r#"
CREATE TABLE IF NOT EXISTS events (
  id         TEXT PRIMARY KEY,
  topic      TEXT NOT NULL,
  author     TEXT NOT NULL,
  wall_ms    INTEGER NOT NULL,
  counter    INTEGER NOT NULL,
  node       TEXT NOT NULL,
  kind       TEXT NOT NULL,
  nonce      BLOB NOT NULL,
  payload    BLOB NOT NULL,
  signature  BLOB NOT NULL
);

-- Merge order, and the scan anti-entropy does.
CREATE INDEX IF NOT EXISTS idx_events_hlc ON events(wall_ms, counter, node);
-- One range per author, for catching a peer up.
CREATE INDEX IF NOT EXISTS idx_events_author ON events(author, wall_ms, counter, node);
CREATE INDEX IF NOT EXISTS idx_events_topic ON events(topic, wall_ms, counter, node);
"#;

/// This device's identity: one row, generated on first open, never replicated
/// and never leaving the device. There is no login, so this *is* the account.
const SCHEMA_V3: &str = r#"
CREATE TABLE IF NOT EXISTS device_keys (
  id     INTEGER PRIMARY KEY CHECK (id = 1),
  secret BLOB NOT NULL
);
"#;

/// Events name the entry they are about, so the projection can group them (and
/// a peer can filter) without the group key. Added by `ALTER` rather than by
/// editing V2, so a database created by an older build gets it the same way.
const SCHEMA_V4: &str = r#"
ALTER TABLE events ADD COLUMN entry_id TEXT NOT NULL DEFAULT '';
CREATE INDEX IF NOT EXISTS idx_events_entry ON events(entry_id, wall_ms, counter, node);

CREATE TABLE IF NOT EXISTS group_keys (
  topic TEXT PRIMARY KEY,
  key   BLOB NOT NULL
);
"#;

/// Device settings: preferences that belong to *this* device and are never
/// replicated. The theme lives here rather than on the baby because one phone's
/// dark mode is nobody else's business.
const SCHEMA_V5: &str = r#"
CREATE TABLE IF NOT EXISTS device_settings (
  id    INTEGER PRIMARY KEY CHECK (id = 1),
  theme TEXT NOT NULL DEFAULT 'auto'
);
INSERT OR IGNORE INTO device_settings (id) VALUES (1);

-- Theme moved here from the profile: it is a device preference, so it must not
-- travel with the baby.
ALTER TABLE babies DROP COLUMN theme;
"#;

/// Which child this device is looking at. A device preference, exactly like the
/// theme: it never replicates, so one phone can follow one child while another
/// follows a sibling.
///
/// The two `UPDATE`s are the upgrade path for a log that predates the intro
/// screen. Entries written when a log could only hold one child are attached to
/// the child that exists, and the device is pointed at it, so an existing user
/// is never asked to "add a baby" they already have.
const SCHEMA_V6: &str = r#"
ALTER TABLE device_settings ADD COLUMN current_baby_id TEXT;

UPDATE entries SET baby_id = (
  SELECT id FROM babies WHERE deleted = 0 ORDER BY created_at, id LIMIT 1
) WHERE baby_id IS NULL;

UPDATE device_settings SET current_baby_id = (
  SELECT id FROM babies WHERE deleted = 0 ORDER BY created_at, id LIMIT 1
) WHERE current_baby_id IS NULL;
"#;

/// Dark became the default appearance after V5 had already shipped `auto`.
///
/// A device still reading `auto` never chose a theme — it inherited that
/// default — so it moves to dark, and the device that set `auto` on purpose
/// re-picks it. Nothing is published either way: the theme is local.
const SCHEMA_V7: &str = r#"
UPDATE device_settings SET theme = 'dark' WHERE theme = 'auto';
"#;

/// Who this device can sync with, per child.
///
/// Learned from an invite (the inviter's address) and from peers that connect to
/// us (the connector's). One row per address, so a device that moves networks
/// simply gains a second one rather than replacing the first.
const SCHEMA_V8: &str = r#"
CREATE TABLE IF NOT EXISTS peers (
  topic TEXT NOT NULL,
  addr  TEXT NOT NULL,
  PRIMARY KEY (topic, addr)
);
"#;

/// Devices asking to join a child's log, waiting for someone here to say yes.
///
/// An invite link no longer carries the group key (see `invite.rs`), so a joiner
/// can reach us and ask but cannot read anything until this table says
/// `accepted`. One row per device per child: a joiner polls, so re-asking must
/// not overwrite a decision that has already been made.
const SCHEMA_V9: &str = r#"
CREATE TABLE IF NOT EXISTS join_requests (
  topic        TEXT NOT NULL,
  device       TEXT NOT NULL,
  role         TEXT NOT NULL,
  requested_at INTEGER NOT NULL,
  state        TEXT NOT NULL DEFAULT 'pending',
  PRIMARY KEY (topic, device)
);
"#;

/// An invite link waiting to be redeemed.
#[derive(Clone, Debug, PartialEq)]
pub struct Invite {
    pub token_hash: String,
    pub role: Role,
    pub invited_by: String,
    pub created_at: Millis,
    pub expires_at: Millis,
    pub accepted_by: Option<String>,
}

impl Invite {
    pub fn is_expired(&self, now: Millis) -> bool {
        self.expires_at <= now
    }

    pub fn is_redeemed(&self) -> bool {
        self.accepted_by.is_some()
    }
}

/// A device waiting to be let into a child's log.
///
/// Serializable because the host's UI lists these, and the joiner polls on the
/// state they carry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JoinRequest {
    pub topic: String,
    /// The asking device's id — the hex of its public key.
    pub device: String,
    /// The role its invite offered. The host may accept or decline; it does not
    /// get to pick a different role, so what the inviter promised is what the
    /// joiner is told.
    pub role: Role,
    pub requested_at: Millis,
    pub state: JoinState,
}

/// A handle on one SQLite database.
pub struct Repo {
    conn: Connection,
}

impl Repo {
    /// Open (creating if needed) a database file, with WAL and a busy timeout.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| CoreError::io(parent.display().to_string(), error))?;
        }
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// An ephemeral database, for tests and previews.
    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;",
        )?;
        let repo = Self { conn };
        repo.migrate()?;
        Ok(repo)
    }

    /// Apply any pending schema migrations (`PRAGMA user_version`).
    ///
    /// Each step is applied in order, so a database created by an older build
    /// catches up rather than being recreated.
    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            self.conn.execute_batch(SCHEMA_V1)?;
        }
        if version < 2 {
            self.conn.execute_batch(SCHEMA_V2)?;
        }
        if version < 3 {
            self.conn.execute_batch(SCHEMA_V3)?;
        }
        if version < 4 {
            self.conn.execute_batch(SCHEMA_V4)?;
        }
        if version < 5 {
            self.conn.execute_batch(SCHEMA_V5)?;
        }
        if version < 6 {
            self.conn.execute_batch(SCHEMA_V6)?;
            // A profile write is sealed with the child's group key, and rows
            // that predate the key table (or came from `ensure_baby`) have
            // none. Minting one is safe: they hold no sealed events yet.
            self.ensure_group_keys()?;
        }
        if version < 7 {
            self.conn.execute_batch(SCHEMA_V7)?;
        }
        if version < 8 {
            self.conn.execute_batch(SCHEMA_V8)?;
        }
        if version < 9 {
            self.conn.execute_batch(SCHEMA_V9)?;
        }
        if version < SCHEMA_VERSION {
            self.conn
                .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        }
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// The append-only log this database's projection is built from.
    pub fn log(&self) -> EventLog<'_> {
        EventLog::new(&self.conn)
    }

    /// This device's identity: loaded on later launches, generated on the first.
    ///
    /// There is no login — see [`crate::keystore`] for what that means when the
    /// key is lost.
    pub fn device_key(&self) -> Result<DeviceKey> {
        keystore::device_key(&SqliteKeyStore::new(&self.conn))
    }

    /* ------------------------------------------------------------ group keys */

    /// The group key for a baby, as handed over by whatever invited this device.
    ///
    /// Kept beside the log the same way the log's payloads are sealed with it —
    /// without it this device can read headers but not one entry.
    pub fn group_key(&self, topic: &str) -> Result<Option<GroupKey>> {
        let stored: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT key FROM group_keys WHERE topic = ?1",
                params![topic],
                |row| row.get(0),
            )
            .optional()?;

        match stored {
            None => Ok(None),
            Some(bytes) => bytes
                .try_into()
                .map(|key| Some(GroupKey::from_bytes(key)))
                .map_err(|_| CoreError::data("the stored group key is not 32 bytes")),
        }
    }

    /* ---------------------------------------------------------------- peers */

    /// Remember one way to reach a peer for a child. Re-remembering is a no-op.
    pub fn save_peer(&self, topic: &str, addr: &str) -> Result<()> {
        if addr.trim().is_empty() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO peers (topic, addr) VALUES (?1, ?2)",
            params![topic, addr],
        )?;
        Ok(())
    }

    /// Every peer address for one child.
    pub fn peers_for(&self, topic: &str) -> Result<Vec<String>> {
        let mut statement = self
            .conn
            .prepare("SELECT addr FROM peers WHERE topic = ?1 ORDER BY addr")?;
        let rows = statement.query_map(params![topic], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    /* -------------------------------------------------------- join requests */

    /// Record that a device wants to join a child's log.
    ///
    /// Idempotent, and deliberately so: a joiner polls until someone answers, so
    /// re-asking must not push a request that was already accepted back to
    /// pending.
    pub fn save_join_request(&self, topic: &str, device: &str, role: Role) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO join_requests (topic, device, role, requested_at, state)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                topic,
                device,
                role.as_str(),
                now_millis(),
                JoinState::Pending.as_str()
            ],
        )?;
        Ok(())
    }

    /// Every request for one child, oldest first — what the host's UI lists.
    pub fn join_requests(&self, topic: &str) -> Result<Vec<JoinRequest>> {
        let mut statement = self.conn.prepare(
            "SELECT topic, device, role, requested_at, state
               FROM join_requests WHERE topic = ?1 ORDER BY requested_at, device",
        )?;
        let rows = statement.query_map(params![topic], join_request_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    /// Where one device's request has got to, or `None` if we never heard it.
    pub fn join_state(&self, topic: &str, device: &str) -> Result<Option<JoinState>> {
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT state FROM join_requests WHERE topic = ?1 AND device = ?2",
                params![topic, device],
                |row| row.get(0),
            )
            .optional()?;
        match stored {
            None => Ok(None),
            Some(value) => JoinState::parse(&value).map(Some),
        }
    }

    /// Accept or decline a request. Accepting is what lets the joiner be handed
    /// the child's key on its next poll.
    pub fn resolve_join_request(&self, topic: &str, device: &str, state: JoinState) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE join_requests SET state = ?3 WHERE topic = ?1 AND device = ?2",
            params![topic, device, state.as_str()],
        )?;
        if changed == 0 {
            return Err(CoreError::NotFound(format!(
                "{device} has not asked to join"
            )));
        }
        Ok(())
    }

    /// Forget an answered request once its answer has been delivered.
    ///
    /// A decline answers one attempt, not every future one. The joiner stops
    /// polling when it hears it, so clearing the row lets a later attempt start
    /// from pending instead of replaying the same rejection forever.
    pub fn clear_join_request(&self, topic: &str, device: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM join_requests WHERE topic = ?1 AND device = ?2",
            params![topic, device],
        )?;
        Ok(())
    }

    /// Every peer address this device knows, across all children.
    pub fn peers(&self) -> Result<Vec<String>> {
        let mut statement = self
            .conn
            .prepare("SELECT DISTINCT addr FROM peers ORDER BY addr")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    /// Every topic this device holds a key for — the children it takes part in,
    /// and therefore exactly what a sync round should exchange.
    pub fn topics(&self) -> Result<Vec<String>> {
        let mut statement = self
            .conn
            .prepare("SELECT topic FROM group_keys ORDER BY topic")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    /// Store the group key for a baby: when this device creates one, or when it
    /// redeems an invite.
    pub fn save_group_key(&self, topic: &str, key: &GroupKey) -> Result<()> {
        self.conn.execute(
            "INSERT INTO group_keys (topic, key) VALUES (?1, ?2)
             ON CONFLICT(topic) DO UPDATE SET key = excluded.key",
            params![topic, &key.to_bytes()[..]],
        )?;
        Ok(())
    }

    /* ----------------------------------------------------------------- baby */

    /// Every live baby, oldest first.
    ///
    /// Empty only before the first profile: the log is the source of truth, so
    /// babies appear here as their profile events are folded in.
    pub fn babies(&self) -> Result<Vec<Baby>> {
        let mut statement = self.conn.prepare(
            "SELECT id, name, born FROM babies WHERE deleted = 0 ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], baby_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    /// The first live baby, for callers that predate multiple babies.
    pub fn baby(&self) -> Result<Option<Baby>> {
        Ok(self.babies()?.into_iter().next())
    }

    /// One baby by id, if it is live.
    pub fn baby_by_id(&self, id: &str) -> Result<Option<Baby>> {
        self.conn
            .query_row(
                "SELECT id, name, born FROM babies WHERE id = ?1 AND deleted = 0",
                params![id],
                baby_from_row,
            )
            .optional()
            .map_err(CoreError::from)
    }

    /// The child this device is looking at.
    ///
    /// The pointer when it still resolves, otherwise the first child there is.
    /// `None` only when the log holds no children at all — which is exactly
    /// when the app should ask for one instead of opening onto a log.
    pub fn current_baby(&self) -> Result<Option<Baby>> {
        match self.pointed_baby()? {
            Some(baby) => Ok(Some(baby)),
            None => self.baby(),
        }
    }

    /// The child the device pointer names, if it still exists.
    fn pointed_baby(&self) -> Result<Option<Baby>> {
        let id: Option<String> = self.conn.query_row(
            "SELECT current_baby_id FROM device_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        match id {
            Some(id) => self.baby_by_id(&id),
            None => Ok(None),
        }
    }

    /// Point this device at a child.
    ///
    /// Device-local, like the theme: switching child on this phone must not
    /// change what any other phone shows.
    pub fn set_current_baby(&self, baby_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE device_settings SET current_baby_id = ?1 WHERE id = 1",
            params![baby_id],
        )?;
        Ok(())
    }

    /// Give every child a group key if it has none.
    ///
    /// A profile write is sealed with this key, so a child without one can be
    /// listed but not edited. Rows that predate the key table have no sealed
    /// events, so minting a key for them loses nothing.
    fn ensure_group_keys(&self) -> Result<()> {
        for baby in self.babies()? {
            let Some(id) = baby.id.as_deref() else {
                continue;
            };
            if self.group_key(id)?.is_none() {
                self.save_group_key(id, &GroupKey::generate()?)?;
            }
        }
        Ok(())
    }

    /// The first baby of this household, created if there is none.
    ///
    /// A convenience for callers that predate the intro screen; the app now
    /// creates children explicitly with [`Repo::create_baby`], so that the user
    /// names them rather than finding one called "Baby".
    pub fn ensure_baby(&self, name: &str) -> Result<Baby> {
        if let Some(baby) = self.baby()? {
            return Ok(baby);
        }
        let now = now_millis();
        let id = crate::model::new_id();
        self.conn.execute(
            "INSERT INTO babies (id, name, born, created_at, updated_at, deleted)
             VALUES (?1, ?2, '', ?3, ?3, 0)",
            params![id, name, now],
        )?;
        // Every child needs a key, because a profile write is sealed with it.
        self.save_group_key(&id, &GroupKey::generate()?)?;
        self.baby_by_id(&id)?
            .ok_or_else(|| CoreError::data("baby missing after insert"))
    }

    /// Save one baby's columns. A baby with no id is a new one.
    ///
    /// This writes the projection's row only; a change that should reach other
    /// devices has to go through [`Repo::save_profile`], which appends an event.
    pub fn save_baby(&self, baby: &Baby) -> Result<Baby> {
        let now = now_millis();
        let id = baby.id.clone().unwrap_or_else(crate::model::new_id);
        self.conn.execute(
            "INSERT INTO babies (id, name, born, created_at, updated_at, deleted)
             VALUES (?1, ?2, ?3, ?4, ?4, 0)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                born = excluded.born,
                updated_at = excluded.updated_at",
            params![id, baby.name, baby.born, now],
        )?;
        self.baby_by_id(&id)?
            .ok_or_else(|| CoreError::data("baby missing after save"))
    }

    /* ------------------------------------------------------ device settings */

    /// This device's theme. A device preference: never replicated, never sent.
    pub fn device_theme(&self) -> Result<Theme> {
        let stored: String = self.conn.query_row(
            "SELECT theme FROM device_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        crate::validate::parse_theme(&stored)
    }

    /// Set this device's theme.
    pub fn save_device_theme(&self, theme: Theme) -> Result<()> {
        self.conn.execute(
            "UPDATE device_settings SET theme = ?1 WHERE id = 1",
            params![theme.as_str()],
        )?;
        Ok(())
    }

    /* --------------------------------------------------------------- entries */

    /// Every live entry, newest first (what the screens render).
    pub fn entries(&self) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM entries WHERE deleted = 0 ORDER BY occurred_at DESC, id DESC"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([], entry_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    /// Every live entry for one baby, newest first.
    pub fn entries_for(&self, baby_id: &str) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM entries
              WHERE deleted = 0 AND baby_id = ?1 ORDER BY occurred_at DESC, id DESC"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![baby_id], entry_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    /// Entries changed after `since`, tombstones included, ordered by
    /// `(updated_at, id)` — the pull half of sync.
    pub fn entries_since(&self, since: Option<&Cursor>) -> Result<Vec<Entry>> {
        let rows = match since {
            Some(cursor) => {
                let sql = format!(
                    "SELECT {ENTRY_COLUMNS} FROM entries
                     WHERE updated_at > ?1 OR (updated_at = ?1 AND id > ?2)
                     ORDER BY updated_at ASC, id ASC"
                );
                let mut statement = self.conn.prepare(&sql)?;
                let rows = statement
                    .query_map(params![cursor.updated_at, cursor.id], entry_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            }
            None => {
                let sql =
                    format!("SELECT {ENTRY_COLUMNS} FROM entries ORDER BY updated_at ASC, id ASC");
                let mut statement = self.conn.prepare(&sql)?;
                let rows = statement
                    .query_map([], entry_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            }
        };
        Ok(rows)
    }

    pub fn entry(&self, id: &str) -> Result<Option<Entry>> {
        let sql = format!("SELECT {ENTRY_COLUMNS} FROM entries WHERE id = ?1");
        self.conn
            .query_row(&sql, params![id], entry_from_row)
            .optional()
            .map_err(CoreError::from)
    }

    /// The newest photo bytes an entry's events carry, if any.
    ///
    /// `photos/<entry>.jpg` is local to the device that took the photo. A peer
    /// that received the entry has the bytes inside the event payload instead,
    /// so the UI can be served from either source.
    pub fn photo_bytes(&self, entry_id: &str) -> Result<Option<Vec<u8>>> {
        let events = self.log().for_entry(entry_id)?;
        let Some(newest) = events.last() else {
            return Ok(None);
        };
        let Some(group) = self.group_key(&newest.topic)? else {
            return Ok(None);
        };

        for event in events.iter().rev() {
            let Ok(plain) = event.open(&group) else {
                continue;
            };
            let Ok(payload) = crate::payload::EntryPayload::from_bytes(&plain) else {
                continue;
            };
            if let Some(encoded) = payload.photo_data {
                if let Some(bytes) = crate::crypto::from_hex(&encoded) {
                    return Ok(Some(bytes));
                }
            }
        }
        Ok(None)
    }

    /// Store a locally-made entry: validated, then last-write-wins.
    /// Returns `false` when the stored row is newer.
    pub fn save_entry(&self, entry: &Entry) -> Result<bool> {
        crate::validate::validate_now(entry)?;
        self.merge_entry(entry)
    }

    /// Store an entry that arrived over sync. Clock skew is tolerated, so a
    /// remote `occurred_at` slightly in the future is not rejected.
    pub fn save_remote_entry(&self, entry: &Entry) -> Result<bool> {
        let now = now_millis().max(entry.occurred_at);
        crate::validate::validate_entry(entry, now)?;
        self.merge_entry(entry)
    }

    fn merge_entry(&self, entry: &Entry) -> Result<bool> {
        let existing = self.entry(&entry.id)?;
        match sync::merge(existing.as_ref(), entry) {
            None => Ok(false),
            Some(winner) => {
                self.write_entry(&winner)?;
                Ok(true)
            }
        }
    }

    /// Upsert without the LWW check (seeding, imports, tests).
    pub fn force_entry(&self, entry: &Entry) -> Result<()> {
        self.write_entry(entry)
    }

    fn write_entry(&self, entry: &Entry) -> Result<()> {
        self.conn.execute(
            "INSERT INTO entries (id, baby_id, kind, occurred_at, ended_at, feed_kind, amount,
                amount_unit, diaper_type, pee_shade, pee_amount, poop_color, consistency, poop_amount,
                sleep_location, wake_mood, note, photo, caregiver_id, created_at, updated_at, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
             ON CONFLICT(id) DO UPDATE SET
                baby_id = excluded.baby_id,
                kind = excluded.kind,
                occurred_at = excluded.occurred_at,
                ended_at = excluded.ended_at,
                feed_kind = excluded.feed_kind,
                amount = excluded.amount,
                amount_unit = excluded.amount_unit,
                diaper_type = excluded.diaper_type,
                pee_shade = excluded.pee_shade,
                pee_amount = excluded.pee_amount,
                poop_color = excluded.poop_color,
                consistency = excluded.consistency,
                poop_amount = excluded.poop_amount,
                sleep_location = excluded.sleep_location,
                wake_mood = excluded.wake_mood,
                note = excluded.note,
                photo = excluded.photo,
                caregiver_id = excluded.caregiver_id,
                -- `created_at` is derived (the earliest write), so a later
                -- arrival of an older event has to be able to move it back.
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                deleted = excluded.deleted",
            params![
                entry.id,
                entry.baby_id,
                entry.kind.as_str(),
                entry.occurred_at,
                entry.ended_at,
                entry.feed_kind.map(|k| k.as_str()),
                entry.amount,
                entry.amount_unit.map(|u| u.as_str()),
                entry.diaper_type.map(|t| t.as_str()),
                entry.pee_shade.map(|s| s.as_str()),
                entry.pee_amount.map(|a| a.as_str()),
                entry.poop_color.map(|c| c.as_str()),
                entry.consistency.map(|c| c.as_str()),
                entry.poop_amount.map(|a| a.as_str()),
                entry.sleep_location.map(|l| l.as_str()),
                entry.wake_mood.map(|m| m.as_str()),
                entry.note,
                entry.photo,
                entry.caregiver_id,
                entry.created_at,
                entry.updated_at,
                i64::from(entry.deleted),
            ],
        )?;
        Ok(())
    }

    /// The sleep that is still running, if the baby is asleep.
    pub fn running_sleep(&self) -> Result<Option<Entry>> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM entries
              WHERE kind = 'sleep' AND ended_at IS NULL AND deleted = 0
              ORDER BY occurred_at DESC LIMIT 1"
        );
        self.conn
            .query_row(&sql, [], entry_from_row)
            .optional()
            .map_err(CoreError::from)
    }

    /// The sleep that is still running for one baby, if that baby is asleep.
    pub fn running_sleep_for(&self, baby_id: &str) -> Result<Option<Entry>> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM entries
              WHERE kind = 'sleep' AND ended_at IS NULL AND deleted = 0 AND baby_id = ?1
              ORDER BY occurred_at DESC LIMIT 1"
        );
        self.conn
            .query_row(&sql, params![baby_id], entry_from_row)
            .optional()
            .map_err(CoreError::from)
    }

    /// Soft-delete one entry (tombstone, so other devices converge).
    pub fn soft_delete(&self, id: &str, caregiver_id: Option<&str>) -> Result<Entry> {
        let mut entry = self
            .entry(id)?
            .ok_or_else(|| CoreError::NotFound(id.to_string()))?;
        entry.deleted = true;
        entry.updated_at = now_millis();
        if let Some(caregiver_id) = caregiver_id {
            entry.caregiver_id = Some(caregiver_id.to_string());
        }
        self.write_entry(&entry)?;
        Ok(entry)
    }

    /// Tombstone every live entry ("delete everything"). Returns how many.
    pub fn delete_all(&self, caregiver_id: Option<&str>) -> Result<usize> {
        let now = now_millis();
        let changed = self.conn.execute(
            "UPDATE entries
                SET deleted = 1, updated_at = ?1, caregiver_id = COALESCE(?2, caregiver_id)
              WHERE deleted = 0",
            params![now, caregiver_id],
        )?;
        Ok(changed)
    }

    pub fn latest_cursor(&self) -> Result<Option<Cursor>> {
        let row: Option<(Millis, String)> = self
            .conn
            .query_row(
                "SELECT updated_at, id FROM entries ORDER BY updated_at DESC, id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(row.map(|(updated_at, id)| Cursor::new(updated_at, id)))
    }

    pub fn count_entries(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE deleted = 0",
                [],
                |row| row.get(0),
            )
            .map_err(CoreError::from)
    }

    /* ------------------------------------------------------------ caregivers */

    pub fn caregivers(&self) -> Result<Vec<Caregiver>> {
        let mut statement = self.conn.prepare(
            "SELECT id, baby_id, name, email, role, created_at, updated_at
               FROM caregivers WHERE deleted = 0 ORDER BY created_at",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Caregiver {
                id: Some(row.get(0)?),
                baby_id: row.get(1)?,
                name: row.get(2)?,
                email: row.get(3)?,
                password_hash: None,
                role: parse_req(row.get::<_, String>(4)?, 4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(CoreError::from)
    }

    pub fn caregiver(&self, id: &str) -> Result<Option<Caregiver>> {
        self.conn
            .query_row(
                "SELECT id, baby_id, name, email, role, created_at, updated_at
                   FROM caregivers WHERE id = ?1 AND deleted = 0",
                params![id],
                |row| {
                    Ok(Caregiver {
                        id: Some(row.get(0)?),
                        baby_id: row.get(1)?,
                        name: row.get(2)?,
                        email: row.get(3)?,
                        password_hash: None,
                        role: parse_req(row.get::<_, String>(4)?, 4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(CoreError::from)
    }

    pub fn caregiver_by_email(&self, email: &str) -> Result<Option<Caregiver>> {
        let id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM caregivers WHERE email = ?1 AND deleted = 0",
                params![email],
                |row| row.get(0),
            )
            .optional()?;
        match id {
            Some(id) => self.caregiver(&id),
            None => Ok(None),
        }
    }

    /// Insert or update a caregiver (keeping any stored password hash).
    pub fn save_caregiver(&self, caregiver: &Caregiver) -> Result<Caregiver> {
        let now = now_millis();
        let id = caregiver
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        self.conn.execute(
            "INSERT INTO caregivers (id, baby_id, name, email, password_hash, role, created_at, updated_at, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 0)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                email = excluded.email,
                role = excluded.role,
                password_hash = COALESCE(excluded.password_hash, caregivers.password_hash),
                updated_at = excluded.updated_at",
            params![
                id,
                caregiver.baby_id,
                caregiver.name,
                caregiver.email,
                caregiver.password_hash,
                caregiver.role.as_str(),
                now,
            ],
        )?;
        self.caregiver(&id)?
            .ok_or_else(|| CoreError::data("caregiver missing after save"))
    }

    /// The stored Argon2id hash for an email, for sign-in.
    pub fn password_hash_for(&self, email: &str) -> Result<Option<(String, Caregiver)>> {
        let row: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT id, password_hash FROM caregivers
                  WHERE email = ?1 AND deleted = 0 AND password_hash IS NOT NULL",
                params![email],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match row {
            Some((id, hash)) => Ok(self.caregiver(&id)?.map(|caregiver| (hash, caregiver))),
            None => Ok(None),
        }
    }

    pub fn count_caregivers(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM caregivers WHERE deleted = 0",
                [],
                |row| row.get(0),
            )
            .map_err(CoreError::from)
    }

    /* --------------------------------------------------------------- invites */

    pub fn create_invite(
        &self,
        token_hash: &str,
        role: Role,
        invited_by: &str,
        expires_at: Millis,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO invites (token_hash, role, invited_by, created_at, expires_at, accepted_by)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            params![token_hash, role.as_str(), invited_by, now_millis(), expires_at],
        )?;
        Ok(())
    }

    pub fn invite(&self, token_hash: &str) -> Result<Option<Invite>> {
        self.conn
            .query_row(
                "SELECT token_hash, role, invited_by, created_at, expires_at, accepted_by
                   FROM invites WHERE token_hash = ?1",
                params![token_hash],
                |row| {
                    Ok(Invite {
                        token_hash: row.get(0)?,
                        role: parse_req(row.get::<_, String>(1)?, 1)?,
                        invited_by: row.get(2)?,
                        created_at: row.get(3)?,
                        expires_at: row.get(4)?,
                        accepted_by: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(CoreError::from)
    }

    /// Redeem an invite once. Returns the role it grants.
    pub fn accept_invite(&self, token_hash: &str, caregiver_id: &str) -> Result<Role> {
        let invite = self
            .invite(token_hash)?
            .ok_or_else(|| CoreError::invalid("that invite link is not valid"))?;
        if invite.is_redeemed() {
            return Err(CoreError::invalid("that invite link has already been used"));
        }
        if invite.is_expired(now_millis()) {
            return Err(CoreError::invalid("that invite link has expired"));
        }
        let changed = self.conn.execute(
            "UPDATE invites SET accepted_by = ?2 WHERE token_hash = ?1 AND accepted_by IS NULL",
            params![token_hash, caregiver_id],
        )?;
        if changed == 0 {
            return Err(CoreError::invalid("that invite link has already been used"));
        }
        Ok(invite.role)
    }

    /// Remove invite links that have been used or expired.
    pub fn prune_invites(&self) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM invites WHERE accepted_by IS NOT NULL OR expires_at <= ?1",
            params![now_millis()],
        )?;
        Ok(deleted)
    }

    /* -------------------------------------------------------------- sessions */

    pub fn create_session(
        &self,
        token_hash: &str,
        caregiver_id: &str,
        expires_at: Millis,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (token_hash, caregiver_id, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![token_hash, caregiver_id, now_millis(), expires_at],
        )?;
        Ok(())
    }

    /// The caregiver for a live session hash, if any.
    pub fn session_caregiver(&self, token_hash: &str) -> Result<Option<Caregiver>> {
        let id: Option<String> = self
            .conn
            .query_row(
                "SELECT caregiver_id FROM sessions WHERE token_hash = ?1 AND expires_at > ?2",
                params![token_hash, now_millis()],
                |row| row.get(0),
            )
            .optional()?;
        match id {
            Some(id) => self.caregiver(&id),
            None => Ok(None),
        }
    }

    pub fn delete_session(&self, token_hash: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sessions WHERE token_hash = ?1",
            params![token_hash],
        )?;
        Ok(())
    }

    /// Drop expired sessions; call on a schedule.
    pub fn prune_sessions(&self) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM sessions WHERE expires_at <= ?1",
            params![now_millis()],
        )?;
        Ok(deleted)
    }
}

/* ------------------------------------------------------------------ mapping */

/// A stored enum value that no longer parses, reported rather than skipped: a
/// request nobody can read is a request nobody can act on, and silently
/// dropping it would look like the joiner never asked.
fn bad_value(index: usize, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

/// Read one `join_requests` row.
fn join_request_from_row(row: &Row<'_>) -> rusqlite::Result<JoinRequest> {
    let role: String = row.get(2)?;
    let state: String = row.get(4)?;
    Ok(JoinRequest {
        topic: row.get(0)?,
        device: row.get(1)?,
        role: Role::parse(&role).map_err(|error| bad_value(2, error.to_string()))?,
        requested_at: row.get(3)?,
        state: JoinState::parse(&state).map_err(|error| bad_value(4, error.to_string()))?,
    })
}

/// Read `SELECT id, name, born` into a [`Baby`].
fn baby_from_row(row: &Row<'_>) -> rusqlite::Result<Baby> {
    Ok(Baby {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        born: row.get(2)?,
    })
}

fn entry_from_row(row: &Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        baby_id: row.get(1)?,
        kind: parse_req(row.get::<_, String>(2)?, 2)?,
        occurred_at: row.get(3)?,
        ended_at: row.get(4)?,
        feed_kind: parse_opt(row.get(5)?, 5)?,
        amount: row.get(6)?,
        amount_unit: parse_opt(row.get(7)?, 7)?,
        diaper_type: parse_opt(row.get(8)?, 8)?,
        pee_shade: parse_opt(row.get(9)?, 9)?,
        pee_amount: parse_opt(row.get(10)?, 10)?,
        poop_color: parse_opt(row.get(11)?, 11)?,
        consistency: parse_opt(row.get(12)?, 12)?,
        poop_amount: parse_opt(row.get(13)?, 13)?,
        sleep_location: parse_opt(row.get(14)?, 14)?,
        wake_mood: parse_opt(row.get(15)?, 15)?,
        note: row.get(16)?,
        photo: row.get(17)?,
        caregiver_id: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
        deleted: row.get::<_, i64>(21)? != 0,
    })
}

pub(crate) fn parse_req<T>(value: String, column: usize) -> rusqlite::Result<T>
where
    T: FromStr<Err = CoreError>,
{
    T::from_str(&value).map_err(|error| conversion_error(column, error))
}

fn parse_opt<T>(value: Option<String>, column: usize) -> rusqlite::Result<Option<T>>
where
    T: FromStr<Err = CoreError>,
{
    match value {
        None => Ok(None),
        Some(text) => T::from_str(&text)
            .map(Some)
            .map_err(|error| conversion_error(column, error)),
    }
}

pub(crate) fn conversion_error(column: usize, error: CoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(error))
}
