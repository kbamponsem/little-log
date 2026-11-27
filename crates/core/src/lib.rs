//! Little Log core: the shared domain model, validation, permissions, the
//! SQLite repository and the CSV/photo helpers.
//!
//! Shared by every front end, so they can never drift apart.

pub mod clock;
pub mod crypto;
pub mod csv;
pub mod error;
pub mod event;
pub mod invite;
pub mod keystore;
pub mod log;
pub mod model;
pub mod payload;
pub mod pending;
pub mod permissions;
pub mod photo;
pub mod projection;
pub mod record;
pub mod repo;
pub mod sync;
pub mod validate;

pub use clock::{Clock, Hlc};
pub use crypto::{DeviceKey, GroupKey};
pub use error::{CoreError, Result};
pub use event::Event;
pub use keystore::{device_key, KeyStore};
pub use log::EventLog;
pub use model::{
    Baby, Caregiver, DiaperDetails, Entry, EntryKind, FeedDetails, JoinState, MoodSchedule, Role,
    SleepDetails, Theme,
};
pub use payload::EntryPayload;
pub use pending::{DrainReport, PendingAction, WidgetAction, WidgetState};
pub use repo::Repo;

/// Serialise/de-serialise a boolean as `true`/`false` or `0`/`1`, so a value
/// written by SQLite, the frontend or an older build all read the same way.
///
/// Generic wire plumbing rather than domain data, so it lives at the crate
/// root; fields opt in with `#[serde(with = "crate::bool01")]`.
mod bool01 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer>(value: &bool, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(i64::from(*value))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<bool, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Bool01 {
            Bool(bool),
            Int(i64),
            Text(String),
        }

        Ok(match Bool01::deserialize(deserializer)? {
            Bool01::Bool(value) => value,
            Bool01::Int(value) => value != 0,
            Bool01::Text(value) => matches!(value.trim(), "1" | "true"),
        })
    }
}
