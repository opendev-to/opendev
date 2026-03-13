//! Secure credential storage with restrictive file permissions.
//!
//! Credentials are stored in `~/.opendev/auth.json` with mode 0600
//! (owner read/write only). Environment variables take precedence over
//! stored credentials.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use crate::models::HttpError;

/// Map of provider names to their environment variable names.
const ENV_VAR_MAP: &[(&str, &str)] = &[
    ("openai", "OPENAI_API_KEY"),
    ("anthropic", "ANTHROPIC_API_KEY"),
    ("fireworks", "FIREWORKS_API_KEY"),
    ("google", "GOOGLE_API_KEY"),
    ("groq", "GROQ_API_KEY"),
    ("mistral", "MISTRAL_API_KEY"),
    ("deepinfra", "DEEPINFRA_API_KEY"),
    ("openrouter", "OPENROUTER_API_KEY"),
    ("azure", "AZURE_OPENAI_API_KEY"),
];

/// On-disk format for auth.json.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct AuthData {
    #[serde(default)]
    keys: HashMap<String, String>,
    #[serde(default)]
    tokens: HashMap<String, TokenEntry>,
}

/// A stored token with optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenEntry {
    token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
}

/// Status of a provider's credential.
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub provider: String,
    pub has_env_key: bool,
    pub has_stored_key: bool,
    pub env_var: String,
}

/// Secure credential store backed by a JSON file with 0600 permissions.
///
/// Environment variables always take precedence over stored values.
pub struct CredentialStore {
    path: PathBuf,
    cache: Option<AuthData>,
}

impl CredentialStore {
    /// Create a new credential store.
    ///
    /// If `auth_path` is `None`, defaults to `~/.opendev/auth.json`.
    pub fn new(auth_path: Option<PathBuf>) -> Self {
        let path = auth_path.unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".opendev")
                .join("auth.json")
        });
        Self { path, cache: None }
    }

    /// Get API key for a provider. Environment variable takes precedence.
    pub fn get_key(&mut self, provider: &str) -> Option<String> {
        let provider_lower = provider.to_lowercase();

        // Check environment variable first
        if let Some(env_var) = env_var_for_provider(&provider_lower)
            && let Ok(val) = std::env::var(env_var)
            && !val.is_empty()
        {
            return Some(val);
        }

        // Fall back to stored credential
        let data = self.load();
        data.keys.get(&provider_lower).cloned()
    }

    /// Store an API key for a provider.
    pub fn set_key(&mut self, provider: &str, key: &str) -> Result<(), HttpError> {
        let mut data = self.load().clone();
        data.keys.insert(provider.to_lowercase(), key.to_string());
        self.save(&data)?;
        info!("Stored API key for {}", provider);
        Ok(())
    }

    /// Remove a stored API key. Returns `true` if the key existed.
    pub fn remove_key(&mut self, provider: &str) -> Result<bool, HttpError> {
        let mut data = self.load().clone();
        let removed = data.keys.remove(&provider.to_lowercase()).is_some();
        if removed {
            self.save(&data)?;
        }
        Ok(removed)
    }

    /// List all known providers with their credential status.
    pub fn list_providers(&mut self) -> Vec<ProviderStatus> {
        let data = self.load();
        ENV_VAR_MAP
            .iter()
            .map(|&(provider, env_var)| {
                let has_env = std::env::var(env_var)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
                let has_stored = data.keys.contains_key(provider);
                ProviderStatus {
                    provider: provider.to_string(),
                    has_env_key: has_env,
                    has_stored_key: has_stored,
                    env_var: env_var.to_string(),
                }
            })
            .collect()
    }

    /// Store an arbitrary token (e.g., OAuth token for MCP servers).
    pub fn store_token(
        &mut self,
        name: &str,
        token: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<(), HttpError> {
        let mut data = self.load().clone();
        data.tokens.insert(
            name.to_string(),
            TokenEntry {
                token: token.to_string(),
                metadata,
            },
        );
        self.save(&data)
    }

    /// Retrieve a stored token.
    pub fn get_token(&mut self, name: &str) -> Option<String> {
        let data = self.load();
        data.tokens.get(name).map(|e| e.token.clone())
    }

    /// Load credentials from file, caching the result.
    fn load(&mut self) -> &AuthData {
        if let Some(ref cached) = self.cache {
            return cached;
        }

        let data = if self.path.exists() {
            // Verify and tighten permissions
            #[cfg(unix)]
            self.check_permissions();

            match std::fs::read_to_string(&self.path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(e) => {
                    warn!("Failed to load credentials from {:?}: {}", self.path, e);
                    AuthData::default()
                }
            }
        } else {
            AuthData::default()
        };

        self.cache = Some(data);
        // SAFETY: we just set self.cache to Some on the line above
        self.cache.as_ref().expect("cache was just set to Some")
    }

    /// Save credentials with restrictive permissions.
    fn save(&mut self, data: &AuthData) -> Result<(), HttpError> {
        self.cache = Some(data.clone());

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write to temp file, then rename (atomic)
        let tmp_path = self.path.with_extension("tmp");
        let json = serde_json::to_string_pretty(data)?;
        std::fs::write(&tmp_path, &json)?;

        // Set permissions before rename
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
        }

        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    /// Check and tighten file permissions on Unix.
    #[cfg(unix)]
    fn check_permissions(&self) {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&self.path) {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                warn!(
                    "Credential file {:?} has loose permissions ({:o}). Tightening to 0600.",
                    self.path, mode
                );
                let _ =
                    std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }

    /// Get the path to the auth file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl std::fmt::Debug for CredentialStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialStore")
            .field("path", &self.path)
            .finish()
    }
}

/// Look up the environment variable name for a provider.
fn env_var_for_provider(provider: &str) -> Option<&'static str> {
    ENV_VAR_MAP
        .iter()
        .find(|&&(p, _)| p == provider)
        .map(|&(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_for_provider() {
        assert_eq!(env_var_for_provider("openai"), Some("OPENAI_API_KEY"));
        assert_eq!(env_var_for_provider("anthropic"), Some("ANTHROPIC_API_KEY"));
        assert_eq!(env_var_for_provider("unknown"), None);
    }

    #[test]
    fn test_credential_store_set_get() {
        let dir = tempfile::tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        let mut store = CredentialStore::new(Some(auth_path.clone()));

        // Use a provider with no env var to avoid interference from the environment
        assert!(store.get_key("testprovider").is_none());

        store.set_key("testprovider", "sk-test-key-123").unwrap();
        assert_eq!(
            store.get_key("testprovider").as_deref(),
            Some("sk-test-key-123")
        );

        // Verify file was created
        assert!(auth_path.exists());

        // Verify permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&auth_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn test_credential_store_remove() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CredentialStore::new(Some(dir.path().join("auth.json")));

        store.set_key("testprovider", "sk-123").unwrap();
        assert!(store.remove_key("testprovider").unwrap());
        assert!(store.get_key("testprovider").is_none());
        assert!(!store.remove_key("testprovider").unwrap());
    }

    #[test]
    fn test_credential_store_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CredentialStore::new(Some(dir.path().join("auth.json")));

        assert!(store.get_token("mcp-github").is_none());

        store
            .store_token(
                "mcp-github",
                "ghp_abc123",
                Some(serde_json::json!({"scope": "repo"})),
            )
            .unwrap();
        assert_eq!(store.get_token("mcp-github").as_deref(), Some("ghp_abc123"));
    }

    #[test]
    fn test_credential_store_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");

        // Write with one instance
        {
            let mut store = CredentialStore::new(Some(auth_path.clone()));
            store.set_key("anthropic", "sk-ant-123").unwrap();
        }

        // Read with a new instance
        {
            let mut store = CredentialStore::new(Some(auth_path));
            assert_eq!(store.get_key("anthropic").as_deref(), Some("sk-ant-123"));
        }
    }

    #[test]
    fn test_list_providers() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CredentialStore::new(Some(dir.path().join("auth.json")));
        store.set_key("openai", "sk-test").unwrap();

        let providers = store.list_providers();
        assert!(!providers.is_empty());

        let openai = providers.iter().find(|p| p.provider == "openai").unwrap();
        assert!(openai.has_stored_key);
        assert_eq!(openai.env_var, "OPENAI_API_KEY");
    }

    #[test]
    fn test_nonexistent_file() {
        let mut store =
            CredentialStore::new(Some(PathBuf::from("/tmp/nonexistent-dir-12345/auth.json")));
        // Use a provider with no env var to avoid interference
        assert!(store.get_key("testprovider").is_none());
    }
}
