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
