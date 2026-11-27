//! A person with access to a child's log.

use serde::{Deserialize, Serialize};

use super::{Millis, Role};

/// A person with access to the baby's log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Caregiver {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub baby_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    /// Argon2id PHC string. Never leaves the server.
    #[serde(default, skip_serializing)]
    pub password_hash: Option<String>,
    pub role: Role,
    #[serde(default)]
    pub created_at: Millis,
    #[serde(default)]
    pub updated_at: Millis,
}

impl Caregiver {
    pub fn new(name: impl Into<String>, role: Role) -> Self {
        let now = crate::validate::now_millis();
        Self {
            id: Some(uuid::Uuid::now_v7().to_string()),
            baby_id: None,
            name: name.into(),
            email: None,
            password_hash: None,
            role,
            created_at: now,
            updated_at: now,
        }
    }
}
