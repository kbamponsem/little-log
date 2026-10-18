//! Server configuration and the state every handler shares.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use little_log_core::model::Millis;
use little_log_core::Repo;

/// How long a session cookie stays valid.
pub const SESSION_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// How long an invite link stays valid.
pub const INVITE_TTL_MS: Millis = 7 * 24 * 60 * 60 * 1000;
/// Failed sign-ins allowed per address per window.
pub const LOGIN_ATTEMPTS: u32 = 10;
pub const LOGIN_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub struct Config {
    /// HMAC key for session and invite tokens; `SESSION_SECRET` in Docker.
    pub session_secret: String,
    /// Always mark cookies `Secure` (set behind HTTPS).
    pub cookie_secure: bool,
    /// Directory holding the SQLite file and `photos/`.
    pub data_dir: PathBuf,
    /// Built frontend to serve (`web/dist`); missing → API only.
    pub web_dir: PathBuf,
}

impl Config {
    pub fn database(&self) -> PathBuf {
        self.data_dir.join("little-log.sqlite3")
    }

    pub fn photos(&self) -> PathBuf {
        self.data_dir.join("photos")
    }
}

/// Per-address login attempt counter.
struct Attempts {
    count: u32,
    window_started: Instant,
}

/// Cheap to clone (every field is shared), which is what axum's `State`
/// extractor wants.
#[derive(Clone)]
pub struct AppState {
    repo: Arc<Mutex<Repo>>,
    pub config: Arc<Config>,
    attempts: Arc<Mutex<HashMap<String, Attempts>>>,
}

impl AppState {
    /// Open the database next to the photos directory.
    pub fn new(config: Config) -> Result<Self, little_log_core::CoreError> {
        std::fs::create_dir_all(&config.data_dir).map_err(|error| {
            little_log_core::CoreError::io(config.data_dir.display().to_string(), error)
        })?;
        std::fs::create_dir_all(config.photos()).map_err(|error| {
            little_log_core::CoreError::io(config.photos().display().to_string(), error)
        })?;

        Ok(Self {
            repo: Arc::new(Mutex::new(Repo::open(config.database())?)),
            config: Arc::new(config),
            attempts: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// A panicking handler must not wedge the whole server.
    pub fn repo(&self) -> MutexGuard<'_, Repo> {
        self.repo
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /* ------------------------------------------------------- login throttling */

    /// Record a failed sign-in; returns false once the address is over budget.
    pub fn record_failed_login(&self, address: &str) -> bool {
        let mut attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        let entry = attempts.entry(address.to_string()).or_insert(Attempts {
            count: 0,
            window_started: now,
        });
        if now.duration_since(entry.window_started) > LOGIN_WINDOW {
            entry.count = 0;
            entry.window_started = now;
        }
        entry.count += 1;
        entry.count <= LOGIN_ATTEMPTS
    }

    pub fn is_rate_limited(&self, address: &str) -> bool {
        let mut attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        match attempts.get_mut(address) {
            Some(entry) if now.duration_since(entry.window_started) > LOGIN_WINDOW => {
                attempts.remove(address);
                false
            }
            Some(entry) => entry.count > LOGIN_ATTEMPTS,
            None => false,
        }
    }

    /// A successful sign-in clears the counter.
    pub fn clear_attempts(&self, address: &str) {
        self.attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(address);
    }
}
