//! Little Log core: the shared domain model, validation, permissions, the
//! SQLite repository, sync merging and the CSV/photo helpers.
//!
//! Used by both the Tauri app (on-device SQLite) and `little-log-server`
//! (self-hosted sync), so the two can never drift apart.

pub mod csv;
pub mod error;
pub mod model;
pub mod pending;
pub mod permissions;
pub mod photo;
pub mod repo;
pub mod sync;
pub mod validate;

pub use error::{CoreError, Result};
pub use model::{Baby, Caregiver, Entry, EntryKind, Role, Theme};
pub use pending::{DrainReport, PendingAction, WidgetAction, WidgetState};
pub use repo::Repo;
