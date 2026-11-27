//! Device-local preferences that live beside the model types.

use serde::{Deserialize, Serialize};

/// This device's daily mood check-in reminder.
///
/// Never replicated: one phone's reminder time is nobody else's business, the
/// same reason the theme lives beside it in `device_settings`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoodSchedule {
    pub enabled: bool,
    /// Minutes after local midnight (0–1439).
    pub minute: i32,
}

impl Default for MoodSchedule {
    fn default() -> Self {
        Self {
            enabled: false,
            minute: 19 * 60,
        }
    }
}
