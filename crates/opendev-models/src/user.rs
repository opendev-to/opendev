//! User authentication models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents an authenticated user account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub password_hash: String,
    #[serde(default = "Utc::now", with = "crate::datetime_compat")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now", with = "crate::datetime_compat")]
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "user".to_string()
}

impl User {
    /// Create a new user.
    pub fn new(username: String, password_hash: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            username,
            email: None,
            password_hash,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            role: "user".to_string(),
        }
    }

    /// Update the updated_at timestamp.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
#[path = "user_tests.rs"]
mod tests;
