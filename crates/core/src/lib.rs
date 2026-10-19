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
pub use model::{Baby, Caregiver, Entry, EntryKind, JoinState, Role, Theme};
pub use payload::EntryPayload;
pub use pending::{DrainReport, PendingAction, WidgetAction, WidgetState};
pub use repo::Repo;
