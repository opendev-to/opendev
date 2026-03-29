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
