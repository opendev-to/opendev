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
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_user() {
        let dir = tempfile::tempdir().unwrap();
        let store = UserStore::new(dir.path().to_path_buf()).unwrap();

        let user = store
            .create_user("alice", "hashed_pw", Some("alice@example.com"))
            .unwrap();
        assert_eq!(user.username, "alice");
        assert_eq!(user.email.as_deref(), Some("alice@example.com"));
        assert_eq!(user.role, "user");

        let found = store.get_by_username("alice").unwrap();
        assert_eq!(found.id, user.id);
    }

    #[test]
    fn test_create_duplicate_user_fails() {
        let dir = tempfile::tempdir().unwrap();
        let store = UserStore::new(dir.path().to_path_buf()).unwrap();

        store.create_user("bob", "hash1", None).unwrap();
        let result = store.create_user("bob", "hash2", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = UserStore::new(dir.path().to_path_buf()).unwrap();

        let user = store.create_user("carol", "hash", None).unwrap();
        let found = store.get_by_id(user.id).unwrap();
        assert_eq!(found.username, "carol");

        assert!(store.get_by_id(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_update_user() {
        let dir = tempfile::tempdir().unwrap();
        let store = UserStore::new(dir.path().to_path_buf()).unwrap();

        let mut user = store.create_user("dave", "hash", None).unwrap();
        user.email = Some("dave@example.com".to_string());
        user.role = "admin".to_string();
        store.update_user(user.clone()).unwrap();

        let found = store.get_by_username("dave").unwrap();
        assert_eq!(found.email.as_deref(), Some("dave@example.com"));
        assert_eq!(found.role, "admin");
    }

    #[test]
    fn test_delete_user() {
        let dir = tempfile::tempdir().unwrap();
        let store = UserStore::new(dir.path().to_path_buf()).unwrap();

        store.create_user("eve", "hash", None).unwrap();
        assert!(store.delete_user("eve").unwrap());
        assert!(store.get_by_username("eve").is_none());
        assert!(!store.delete_user("eve").unwrap());
    }

    #[test]
    fn test_list_and_count() {
        let dir = tempfile::tempdir().unwrap();
        let store = UserStore::new(dir.path().to_path_buf()).unwrap();

        assert_eq!(store.count(), 0);
        store.create_user("u1", "h1", None).unwrap();
        store.create_user("u2", "h2", None).unwrap();
        assert_eq!(store.count(), 2);

        let mut names = store.list_usernames();
        names.sort();
        assert_eq!(names, vec!["u1", "u2"]);
    }

    #[test]
    fn test_persistence_across_instances() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        // Create a user with first instance
        {
            let store = UserStore::new(path.clone()).unwrap();
            store.create_user("frank", "hash", None).unwrap();
        }

        // Read with a new instance
        {
            let store = UserStore::new(path).unwrap();
            let found = store.get_by_username("frank");
            assert!(found.is_some());
            assert_eq!(found.unwrap().username, "frank");
        }
    }

    #[test]
    fn test_empty_dir_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("nested").join("dir");
        let store = UserStore::new(sub.clone()).unwrap();
        assert!(sub.join("users.json").exists());
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_get_nonexistent_user() {
        let dir = tempfile::tempdir().unwrap();
        let store = UserStore::new(dir.path().to_path_buf()).unwrap();
        assert!(store.get_by_username("nobody").is_none());
    }
}
