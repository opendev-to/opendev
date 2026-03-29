//! File-based user store for authentication.
//!
//! Provides a thread-safe, JSON-backed store for user accounts.
//! Users are stored in `{storage_dir}/users.json` and accessed via
//! username or UUID lookups.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use chrono::Utc;
use tracing::warn;
use uuid::Uuid;

use opendev_models::User;

use crate::models::HttpError;

/// Thread-safe, JSON-backed store for user accounts.
///
/// All mutations are persisted to disk immediately. Read operations
/// use an in-memory cache protected by a `RwLock` so multiple readers
/// can proceed concurrently.
#[derive(Debug)]
pub struct UserStore {
    users_file: PathBuf,
    users: Arc<RwLock<HashMap<String, User>>>,
}

impl UserStore {
    /// Create a new user store backed by `{storage_dir}/users.json`.
    ///
    /// Creates the directory and file if they do not exist.
    pub fn new(storage_dir: PathBuf) -> Result<Self, HttpError> {
        let users_file = storage_dir.join("users.json");
        let store = Self {
            users_file,
            users: Arc::new(RwLock::new(HashMap::new())),
        };
        store.load()?;
        Ok(store)
    }

    /// Look up a user by username.
    pub fn get_by_username(&self, username: &str) -> Option<User> {
        let users = self.users.read().expect("RwLock poisoned");
        users.get(username).cloned()
    }

    /// Look up a user by UUID.
    pub fn get_by_id(&self, user_id: Uuid) -> Option<User> {
        let users = self.users.read().expect("RwLock poisoned");
        users.values().find(|u| u.id == user_id).cloned()
    }

    /// Create a new user account.
    ///
    /// Returns an error if the username is already taken.
    pub fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        email: Option<&str>,
    ) -> Result<User, HttpError> {
        let mut users = self.users.write().expect("RwLock poisoned");
        if users.contains_key(username) {
            return Err(HttpError::Other(format!(
                "User already exists: {}",
                username
            )));
        }
        let mut user = User::new(username.to_string(), password_hash.to_string());
        user.email = email.map(|s| s.to_string());
        users.insert(username.to_string(), user.clone());
        drop(users);
        self.persist()?;
        Ok(user)
    }

    /// Update an existing user record.
    ///
    /// The `updated_at` timestamp is set to now automatically.
    pub fn update_user(&self, mut user: User) -> Result<(), HttpError> {
        let mut users = self.users.write().expect("RwLock poisoned");
        user.updated_at = Utc::now();
        users.insert(user.username.clone(), user);
        drop(users);
        self.persist()
    }

    /// Delete a user by username.
    ///
    /// Returns `true` if the user existed and was removed.
    pub fn delete_user(&self, username: &str) -> Result<bool, HttpError> {
        let mut users = self.users.write().expect("RwLock poisoned");
        let removed = users.remove(username).is_some();
        drop(users);
        if removed {
            self.persist()?;
        }
        Ok(removed)
    }

    /// List all usernames in the store.
    pub fn list_usernames(&self) -> Vec<String> {
        let users = self.users.read().expect("RwLock poisoned");
        users.keys().cloned().collect()
    }

    /// Return the total number of users.
    pub fn count(&self) -> usize {
        let users = self.users.read().expect("RwLock poisoned");
        users.len()
    }

    /// Load users from the JSON file into memory.
    fn load(&self) -> Result<(), HttpError> {
        if !self.users_file.exists() {
            // Create parent dirs and an empty file
            if let Some(parent) = self.users_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // Write to temp file, then rename (atomic)
            let tmp_path = self
                .users_file
                .with_extension(format!("tmp.{}", Uuid::new_v4()));

            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                let mut opts = std::fs::OpenOptions::new();
                opts.write(true).create(true).truncate(true).mode(0o600);
                std::io::Write::write_all(&mut opts.open(&tmp_path)?, b"{}")?;
            }
            #[cfg(not(unix))]
            {
                std::fs::write(&tmp_path, "{}")?;
            }

            std::fs::rename(&tmp_path, &self.users_file)?;
            return Ok(());
        }

        // Verify and tighten permissions
        #[cfg(unix)]
        self.check_permissions();

        match std::fs::read_to_string(&self.users_file) {
            Ok(content) => {
                let parsed: HashMap<String, User> =
                    serde_json::from_str(&content).unwrap_or_else(|e| {
                        warn!("Failed to parse users file {:?}: {}", self.users_file, e);
                        HashMap::new()
                    });
                let mut users = self.users.write().expect("RwLock poisoned");
                *users = parsed;
            }
            Err(e) => {
                warn!("Failed to read users file {:?}: {}", self.users_file, e);
            }
        }
        Ok(())
    }

    /// Persist the in-memory user map to disk.
    fn persist(&self) -> Result<(), HttpError> {
        let users = self.users.read().expect("RwLock poisoned");
        let json = serde_json::to_string_pretty(&*users)?;
        drop(users);

        if let Some(parent) = self.users_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write to temp file, then rename (atomic)
        let tmp_path = self
            .users_file
            .with_extension(format!("tmp.{}", Uuid::new_v4()));

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true).mode(0o600);
            std::io::Write::write_all(&mut opts.open(&tmp_path)?, json.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&tmp_path, json)?;
        }

        std::fs::rename(&tmp_path, &self.users_file)?;
        Ok(())
    }

    /// Check and tighten file permissions on Unix.
    #[cfg(unix)]
    fn check_permissions(&self) {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&self.users_file) {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                warn!(
                    "User store file {:?} has loose permissions ({:o}). Tightening to 0600.",
                    self.users_file, mode
                );
                let _ = std::fs::set_permissions(
                    &self.users_file,
                    std::fs::Permissions::from_mode(0o600),
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "user_store_tests.rs"]
mod tests;
