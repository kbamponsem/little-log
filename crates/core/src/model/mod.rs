//! The domain model, split by concern.
//!
//! Serde ids are snake_case so stored/synced values never depend on localised
//! text — `label()` renders the prototype's wording.
//!
//! `entry` holds the logged rows and their per-kind groups, `enums` the stable
//! value types, and the remaining modules the profile and device records. The
//! public paths do not move: everything is re-exported from here, so
//! `crate::model::Entry` still works.

#[macro_use]
mod macros;

mod baby;
mod caregiver;
mod entry;
mod enums;
mod schedule;
#[cfg(test)]
mod tests;

pub use baby::Baby;
pub use caregiver::Caregiver;
pub use entry::{DiaperDetails, Entry, FeedDetails, SleepDetails};
pub use enums::{
    Amount, AmountUnit, Consistency, DiaperType, EntryKind, EventKind, FeedKind, JoinState, Mood,
    PeeShade, PoopColor, Role, SleepLocation, Theme, WakeMood,
};
pub use schedule::MoodSchedule;

/// Milliseconds since the Unix epoch, UTC.
pub type Millis = i64;

/// A fresh UUIDv7 string — the id every entry row is created with, so writes
/// from the app, the widget and the server all collide-proofly merge.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
