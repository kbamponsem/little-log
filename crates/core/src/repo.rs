//! SQLite repository — the single place that knows the schema.
//!
//! Both the app (one file on the device) and the server (one file in the
//! `/data` volume) open a [`Repo`]. Writes go through last-write-wins so the
//! two can never disagree about an entry.

use std::path::Path;
use std::str::FromStr;

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::crypto::{DeviceKey, GroupKey};
use crate::error::{CoreError, Result};
use crate::keystore::{self, SqliteKeyStore};
use crate::log::EventLog;
use crate::model::{Baby, Caregiver, Entry, Millis, Role};
use crate::sync::{self, Cursor};
use crate::validate::now_millis;

/// Bumped whenever the schema below changes; see [`Repo::migrate`].
pub const SCHEMA_VERSION: i64 = 4;

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

    /* ------------------------------------------------------------------ baby */

    pub fn baby(&self) -> Result<Option<Baby>> {
        self.conn
            .query_row(
                "SELECT id, name, born, theme FROM babies WHERE deleted = 0 ORDER BY created_at LIMIT 1",
                [],
                |row| {
                    Ok(Baby {
                        id: Some(row.get(0)?),
                        name: row.get(1)?,
                        born: row.get(2)?,
                        theme: parse_req(row.get::<_, String>(3)?, 3)?,
                    })
                },
            )
            .optional()
            .map_err(CoreError::from)
    }

    /// The single baby of this household, created on first run.
    pub fn ensure_baby(&self, name: &str) -> Result<Baby> {
        if let Some(baby) = self.baby()? {
            return Ok(baby);
        }
        let now = now_millis();
        self.conn.execute(
            "INSERT INTO babies (id, name, born, theme, created_at, updated_at, deleted)
             VALUES (?1, ?2, '', 'auto', ?3, ?3, 0)",
            params![uuid::Uuid::now_v7().to_string(), name, now],
        )?;
        self.baby()?
            .ok_or_else(|| CoreError::data("baby missing after insert"))
    }

    pub fn save_baby(&self, baby: &Baby) -> Result<Baby> {
        crate::validate::parse_theme(baby.theme.as_str())?;
        let now = now_millis();
        // One baby per household: fall back to the stored row's id so a client
        // that omits it updates rather than inserting a second profile.
        let id = match baby.id.clone() {
            Some(id) => Some(id),
            None => self.baby()?.and_then(|stored| stored.id),
        };
        match &id {
            Some(id) => {
                self.conn.execute(
                    "UPDATE babies SET name = ?2, born = ?3, theme = ?4, updated_at = ?5 WHERE id = ?1",
                    params![id, baby.name, baby.born, baby.theme.as_str(), now],
                )?;
            }
            None => {
                self.conn.execute(
                    "INSERT INTO babies (id, name, born, theme, created_at, updated_at, deleted)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5, 0)",
                    params![
                        crate::model::new_id(),
                        baby.name,
                        baby.born,
                        baby.theme.as_str(),
                        now
                    ],
                )?;
            }
        }
        self.baby()?
            .ok_or_else(|| CoreError::data("baby missing after save"))
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
