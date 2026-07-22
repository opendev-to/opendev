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

    // Use a provider with no env var so a developer's real ANTHROPIC_API_KEY /
    // OPENAI_API_KEY cannot shadow the stored credential via get_key's env lookup.
    // Write with one instance
    {
        let mut store = CredentialStore::new(Some(auth_path.clone()));
        store.set_key("testprovider", "sk-test-123").unwrap();
    }

    // Read with a new instance
    {
        let mut store = CredentialStore::new(Some(auth_path));
        assert_eq!(
            store.get_key("testprovider").as_deref(),
            Some("sk-test-123")
        );
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

#[test]
fn test_chatgpt_oauth_credentials_are_typed_redacted_and_persistent() {
    let dir = tempfile::tempdir().unwrap();
    let auth_path = dir.path().join("auth.json");
    let credential = ChatGptOAuthCredential {
        access_token: "access-secret".to_string(),
        refresh_token: "refresh-secret".to_string(),
        expires_at_ms: 1_700_000_000_000,
        account_id: Some("workspace-123".to_string()),
    };

    let mut store = CredentialStore::new(Some(auth_path.clone()));
    store.store_chatgpt_oauth(credential.clone()).unwrap();
    assert_eq!(store.get_chatgpt_oauth(), Some(credential));
    let debug = format!("{:?}", store.get_chatgpt_oauth().unwrap());
    assert!(!debug.contains("access-secret"));
    assert!(!debug.contains("refresh-secret"));
    assert!(debug.contains("[REDACTED]"));

    let mut reloaded = CredentialStore::new(Some(auth_path));
    assert!(reloaded.get_chatgpt_oauth().is_some());
    assert!(reloaded.remove_chatgpt_oauth().unwrap());
    assert!(reloaded.get_chatgpt_oauth().is_none());
}

#[test]
fn test_chatgpt_oauth_read_bypasses_stale_cache() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");
    let mut first = CredentialStore::new(Some(path.clone()));
    let mut second = CredentialStore::new(Some(path));

    first
        .store_chatgpt_oauth(ChatGptOAuthCredential {
            access_token: "first-access".to_string(),
            refresh_token: "first-refresh".to_string(),
            expires_at_ms: 1,
            account_id: None,
        })
        .unwrap();
    assert_eq!(
        second.get_chatgpt_oauth().unwrap().access_token,
        "first-access"
    );

    first
        .store_chatgpt_oauth(ChatGptOAuthCredential {
            access_token: "rotated-access".to_string(),
            refresh_token: "rotated-refresh".to_string(),
            expires_at_ms: 2,
            account_id: None,
        })
        .unwrap();
    assert_eq!(
        second.get_chatgpt_oauth().unwrap().access_token,
        "rotated-access"
    );
}

#[test]
fn test_chatgpt_oauth_successful_login_replaces_the_previous_credential() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = CredentialStore::new(Some(dir.path().join("auth.json")));

    store
        .store_chatgpt_oauth(ChatGptOAuthCredential {
            access_token: "old-access".to_string(),
            refresh_token: "old-refresh".to_string(),
            expires_at_ms: 1,
            account_id: Some("old-account".to_string()),
        })
        .unwrap();
    store
        .store_chatgpt_oauth(ChatGptOAuthCredential {
            access_token: "new-access".to_string(),
            refresh_token: "new-refresh".to_string(),
            expires_at_ms: 2,
            account_id: Some("new-account".to_string()),
        })
        .unwrap();

    assert_eq!(
        store.get_chatgpt_oauth(),
        Some(ChatGptOAuthCredential {
            access_token: "new-access".to_string(),
            refresh_token: "new-refresh".to_string(),
            expires_at_ms: 2,
            account_id: Some("new-account".to_string()),
        })
    );
}
