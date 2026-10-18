//! Where a device's identity lives.
//!
//! There is no login and no account: a device's ed25519 keypair **is** its
//! identity, generated the first time it opens and kept on the device. Every
//! event it authors is signed with it, and `Event::author` is the hex of its
//! public half — so a peer can verify an event without asking anyone.
//!
//! **Losing the key loses the identity.** The log itself survives on other
//! devices, and events already replicated stay attributed to the old public
//! key (their signatures still verify), but this device can no longer author
//! as it, and any role certificate bound to it has to be re-issued. There is
//! deliberately no recovery path: with no server, nobody could reset it.

use rusqlite::{params, Connection, OptionalExtension};

use crate::crypto::{DeviceKey, SECRET_KEY_LEN};
use crate::error::{CoreError, Result};

/// Somewhere a device's secret can be kept.
///
/// A trait because the app may move to an OS keystore later, and because the
/// headless peer has no OS keystore at all — it needs a file or environment
/// backend. Callers only ever see [`device_key`].
pub trait KeyStore {
    /// The stored secret, if this device has one yet.
    fn load(&self) -> Result<Option<[u8; SECRET_KEY_LEN]>>;
    /// Persist the secret this device just generated.
    fn save(&self, secret: &[u8; SECRET_KEY_LEN]) -> Result<()>;
}

/// This device's key: loaded if there is one, generated and stored on first use.
///
/// A store that cannot save is an error rather than a warning: carrying on with
/// an unpersisted key would mint a different identity on the next launch, and
/// the events authored in between would belong to an identity that vanishes.
pub fn device_key(store: &impl KeyStore) -> Result<DeviceKey> {
    if let Some(secret) = store.load()? {
        return Ok(DeviceKey::from_secret_bytes(&secret));
    }
    let key = DeviceKey::generate()?;
    store.save(&key.secret_bytes())?;
    Ok(key)
}

/// The secret kept in the local database, beside the log it authors.
pub struct SqliteKeyStore<'a> {
    conn: &'a Connection,
}

impl<'a> SqliteKeyStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
}

impl KeyStore for SqliteKeyStore<'_> {
    fn load(&self) -> Result<Option<[u8; SECRET_KEY_LEN]>> {
        // The table holds at most one row, enforced by a CHECK on the id.
        let stored: Option<Vec<u8>> = self
            .conn
            .query_row("SELECT secret FROM device_keys WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?;

        match stored {
            None => Ok(None),
            Some(bytes) => bytes
                .try_into()
                .map(Some)
                .map_err(|_| CoreError::data("the stored device key is not an ed25519 secret")),
        }
    }

    fn save(&self, secret: &[u8; SECRET_KEY_LEN]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO device_keys (id, secret) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET secret = excluded.secret",
            params![&secret[..]],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repo;
    use std::cell::RefCell;

    /// A store with no database behind it, so the identity logic can be tested
    /// on its own — including the case where saving fails.
    #[derive(Default)]
    struct Memory {
        secret: RefCell<Option<[u8; SECRET_KEY_LEN]>>,
        fail_save: bool,
    }

    impl KeyStore for Memory {
        fn load(&self) -> Result<Option<[u8; SECRET_KEY_LEN]>> {
            Ok(*self.secret.borrow())
        }

        fn save(&self, secret: &[u8; SECRET_KEY_LEN]) -> Result<()> {
            if self.fail_save {
                return Err(CoreError::data("no room for the key"));
            }
            *self.secret.borrow_mut() = Some(*secret);
            Ok(())
        }
    }

    #[test]
    fn the_first_open_generates_a_key_and_stores_it() {
        let store = Memory::default();
        assert!(store.load().unwrap().is_none());

        let key = device_key(&store).unwrap();

        assert_eq!(store.load().unwrap(), Some(key.secret_bytes()));
    }

    #[test]
    fn every_later_open_reuses_the_same_identity() {
        let store = Memory::default();

        let first = device_key(&store).unwrap();
        let second = device_key(&store).unwrap();

        assert_eq!(first.id(), second.id());
    }

    #[test]
    fn two_devices_get_different_identities() {
        let (one, two) = (Memory::default(), Memory::default());

        assert_ne!(
            device_key(&one).unwrap().id(),
            device_key(&two).unwrap().id()
        );
    }

    #[test]
    fn a_store_that_cannot_save_fails_rather_than_forgetting_the_key() {
        // Returning a key we could not persist would mean the next launch has a
        // different identity, and this session's events would be unattributable.
        let store = Memory {
            fail_save: true,
            ..Default::default()
        };

        assert!(device_key(&store).is_err());
    }

    #[test]
    fn the_identity_it_hands_out_can_author_verifiable_events() {
        let store = Memory::default();
        let key = device_key(&store).unwrap();

        let signature = key.sign(b"an event");

        assert!(crate::crypto::verify_device(&key.id(), b"an event", &signature).is_ok());
    }

    #[test]
    fn the_database_store_round_trips_and_keeps_one_identity() {
        let repo = Repo::open_in_memory().unwrap();
        let store = SqliteKeyStore::new(repo.connection());
        assert!(store.load().unwrap().is_none());

        let first = device_key(&store).unwrap();
        let second = device_key(&store).unwrap();

        assert_eq!(first.id(), second.id());
        let rows: i64 = repo
            .connection()
            .query_row("SELECT COUNT(*) FROM device_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1, "a device has exactly one identity");
    }
}
