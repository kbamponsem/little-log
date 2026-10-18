//! App-local preferences.
//!
//! Never synced: the sync server address and the session token belong to this
//! device. Everything the family shares lives in SQLite instead.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Sync server URL, e.g. `https://baby.example.com`. Empty = offline only.
    #[serde(default)]
    pub server: String,
    /// Bearer-ish session token for the sync server (never the password).
    #[serde(default)]
    pub session_token: String,
    /// Email the user signed in with, for display.
    #[serde(default)]
    pub email: String,
}

impl Settings {
    /// Read settings, falling back to defaults when the file is missing or
    /// corrupt (a broken file must never stop the app from opening).
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        std::fs::write(path, text)
    }

    pub fn has_server(&self) -> bool {
        !self.server.trim().is_empty()
    }

    pub fn is_signed_in(&self) -> bool {
        self.has_server() && !self.session_token.is_empty()
    }
}
