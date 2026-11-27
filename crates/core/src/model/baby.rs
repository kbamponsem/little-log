//! A child's profile.

use serde::{Deserialize, Serialize};

/// A baby's profile: its name, birthday and picture.
///
/// There is no theme here on purpose. Appearance is a device preference and
/// lives in `device_settings`, so it never travels with the baby.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Baby {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    /// Birthday as `YYYY-MM-DD`, or empty.
    #[serde(default)]
    pub born: String,
    /// File name in `photos/` for the profile picture, or `None`.
    ///
    /// The name travels in the profile event, so a peer knows a picture exists;
    /// the bytes travel with the event too, and are read back on demand.
    #[serde(default)]
    pub photo: Option<String>,
}

impl Default for Baby {
    fn default() -> Self {
        Self {
            id: None,
            name: "Baby".to_string(),
            born: String::new(),
            photo: None,
        }
    }
}
