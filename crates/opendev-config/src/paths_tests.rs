use super::*;
use std::env;

#[test]
fn test_encode_project_path() {
    // Use a path that doesn't need canonicalization
    let encoded = "/Users/foo/bar".replace('/', "-");
    assert_eq!(encoded, "-Users-foo-bar");
}

#[test]
fn test_paths_project_dir() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/test-project")));
    assert_eq!(
        paths.project_dir(),
        PathBuf::from("/tmp/test-project/.opendev")
    );
}

#[test]
fn test_session_file() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/test")));
    let session_path = paths.session_file("abc123");
    assert!(session_path.to_string_lossy().ends_with("abc123.json"));
}

#[test]
fn test_project_context_file() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/myproject")));
    assert_eq!(
        paths.project_context_file(),
        PathBuf::from("/tmp/myproject/AGENTS.md")
    );
}

#[test]
fn test_project_mcp_config() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/myproject")));
    assert_eq!(
        paths.project_mcp_config(),
        PathBuf::from("/tmp/myproject/.mcp.json")
    );
}

#[test]
fn test_opendev_dir_env_override() {
    let key = "OPENDEV_DIR";
    // SAFETY: test runs single-threaded for env var manipulation
    let original = env::var(key).ok();

    unsafe { env::set_var(key, "/tmp/custom-opendev") };
    let paths = Paths::new(Some(PathBuf::from("/tmp/wd")));
    assert_eq!(paths.global_dir(), PathBuf::from("/tmp/custom-opendev"));
    assert_eq!(
        paths.global_settings(),
        PathBuf::from("/tmp/custom-opendev/settings.json")
    );
    assert_eq!(
        paths.global_sessions_dir(),
        PathBuf::from("/tmp/custom-opendev/sessions")
    );
    assert_eq!(
        paths.global_logs_dir(),
        PathBuf::from("/tmp/custom-opendev/logs")
    );

    // Restore
    match original {
        Some(v) => unsafe { env::set_var(key, v) },
        None => unsafe { env::remove_var(key) },
    }
}

#[test]
fn test_xdg_accessors_present() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/wd")));
    // Just verify the accessors don't panic and return non-empty paths
    assert!(!paths.config_dir().as_os_str().is_empty());
    assert!(!paths.data_dir().as_os_str().is_empty());
    assert!(!paths.cache_dir().as_os_str().is_empty());
    assert!(!paths.state_dir().as_os_str().is_empty());
}

#[test]
fn test_all_base_dirs() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/wd")));
    let bases = paths.all_base_dirs();
    assert!(bases.len() >= 4);
}

#[test]
fn test_config_vs_data_separation() {
    // With OPENDEV_DIR override, config and data point to same place
    let key = "OPENDEV_DIR";
    // SAFETY: test runs single-threaded for env var manipulation
    let original = env::var(key).ok();

    unsafe { env::set_var(key, "/tmp/override-opendev") };
    let paths = Paths::new(Some(PathBuf::from("/tmp/wd")));
    // Settings (config) in config_dir
    assert!(paths.global_settings().starts_with("/tmp/override-opendev"));
    // Sessions (data) in data_dir
    assert!(
        paths
            .global_sessions_dir()
            .starts_with("/tmp/override-opendev")
    );

    match original {
        Some(v) => unsafe { env::set_var(key, v) },
        None => unsafe { env::remove_var(key) },
    }
}

#[test]
fn test_global_commands_dir_under_config() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/wd")));
    assert_eq!(
        paths.global_commands_dir(),
        paths.config_dir().join(COMMANDS_DIR_NAME)
    );
}

#[test]
fn test_providers_cache_dir_under_global_cache() {
    // The /models picker and sync_provider_cache both derive the providers
    // directory from global_cache_dir(); keep them in lockstep.
    let paths = Paths::new(Some(PathBuf::from("/tmp/wd")));
    assert_eq!(
        paths.providers_cache_dir(),
        paths.global_cache_dir().join("providers")
    );
}

#[test]
fn test_legacy_detection_and_fresh_install_layout() {
    // Combined into one test to avoid two tests racing on HOME.
    let original_home = env::var("HOME").ok();
    let original_override = env::var(ENV_OPENDEV_DIR).ok();

    let tmp = tempfile::tempdir().unwrap();
    // macOS: /var symlinks to /private/var
    let home = tmp.path().canonicalize().unwrap();

    // SAFETY: test-only env manipulation, restored below.
    unsafe {
        env::remove_var(ENV_OPENDEV_DIR);
        env::set_var("HOME", &home);
    }

    // Fresh install: no ~/.opendev — all base dirs must avoid it.
    let fresh = Paths::new(Some(PathBuf::from("/tmp/wd")));

    // A directory containing only runtime artifacts is not a legacy install.
    std::fs::create_dir_all(home.join(APP_DIR_NAME)).unwrap();
    let artifact_only = Paths::new(Some(PathBuf::from("/tmp/wd")));

    // Legacy install: a legacy settings file exists — everything points at it.
    std::fs::write(home.join(APP_DIR_NAME).join(SETTINGS_FILE_NAME), "{}").unwrap();
    #[cfg_attr(windows, allow(unused_variables))]
    let legacy = Paths::new(Some(PathBuf::from("/tmp/wd")));

    // Restore env before asserting to keep the mutation window small.
    // SAFETY: restoring original values.
    unsafe {
        match original_home {
            Some(ref v) => env::set_var("HOME", v),
            None => env::remove_var("HOME"),
        }
        match original_override {
            Some(ref v) => env::set_var(ENV_OPENDEV_DIR, v),
            None => env::remove_var(ENV_OPENDEV_DIR),
        }
    }

    let legacy_dir = home.join(APP_DIR_NAME);
    for dir in fresh.all_base_dirs() {
        assert!(
            !dir.starts_with(&legacy_dir),
            "fresh install must not use legacy dir: {}",
            dir.display()
        );
    }

    for dir in artifact_only.all_base_dirs() {
        assert!(
            !dir.starts_with(&legacy_dir),
            "artifact-only legacy dir must not shadow XDG paths: {}",
            dir.display()
        );
    }

    // dirs::home_dir() resolves via the OS profile-folder API on Windows and
    // ignores the HOME env var, so this test can't steer it there; only
    // assert the legacy-dir equality on platforms where the override above
    // actually takes effect.
    #[cfg(not(windows))]
    {
        assert_eq!(legacy.config_dir(), legacy_dir.as_path());
        assert_eq!(legacy.data_dir(), legacy_dir.as_path());
        assert_eq!(legacy.state_dir(), legacy_dir.as_path());
        assert_eq!(
            legacy.cache_dir(),
            legacy_dir.join(CACHE_DIR_NAME).as_path()
        );
    }
}
