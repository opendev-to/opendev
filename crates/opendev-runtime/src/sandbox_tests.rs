use super::*;

#[test]
fn test_disabled_allows_everything() {
    let config = SandboxConfig::disabled();
    assert!(config.check_command("rm -rf /").is_ok());
    assert!(config.check_writable_path(Path::new("/etc/passwd")).is_ok());
}

#[test]
fn test_enabled_blocks_unlisted_commands() {
    let config = SandboxConfig {
        enabled: true,
        allowed_commands: vec!["cargo".into(), "ls".into()],
        writable_paths: vec![],
    };
    assert!(config.check_command("cargo build").is_ok());
    assert!(config.check_command("ls -la").is_ok());
    assert!(config.check_command("rm -rf /").is_err());
    assert!(config.check_command("curl http://evil.com").is_err());
}

#[test]
fn test_command_with_env_prefix() {
    let config = SandboxConfig {
        enabled: true,
        allowed_commands: vec!["cargo".into()],
        writable_paths: vec![],
    };
    assert!(config.check_command("RUST_LOG=debug cargo test").is_ok());
}

#[test]
fn test_command_with_full_path() {
    let config = SandboxConfig {
        enabled: true,
        allowed_commands: vec!["cargo".into()],
        writable_paths: vec![],
    };
    assert!(config.check_command("/usr/bin/cargo build").is_ok());
}

#[test]
fn test_writable_path_within_project() {
    let config = SandboxConfig {
        enabled: true,
        allowed_commands: vec![],
        writable_paths: vec!["/home/user/project".into()],
    };
    assert!(
        config
            .check_writable_path(Path::new("/home/user/project/src/main.rs"))
            .is_ok()
    );
    assert!(
        config
            .check_writable_path(Path::new("/home/user/project"))
            .is_ok()
    );
    assert!(
        config
            .check_writable_path(Path::new("/home/user/other/file.rs"))
            .is_err()
    );
    assert!(
        config
            .check_writable_path(Path::new("/etc/passwd"))
            .is_err()
    );
}

#[test]
fn test_for_project_preset() {
    let config = SandboxConfig::for_project(Path::new("/home/user/myapp"));
    assert!(config.enabled);
    assert!(config.check_command("cargo build").is_ok());
    assert!(config.check_command("git status").is_ok());
    assert!(config.check_command("rm -rf /").is_err());
    assert!(
        config
            .check_writable_path(Path::new("/home/user/myapp/src/lib.rs"))
            .is_ok()
    );
    assert!(
        config
            .check_writable_path(Path::new("/home/user/other/file"))
            .is_err()
    );
}

#[test]
fn test_empty_command() {
    let config = SandboxConfig {
        enabled: true,
        allowed_commands: vec![],
        writable_paths: vec![],
    };
    assert!(config.check_command("").is_ok());
    assert!(config.check_command("   ").is_ok());
}

#[test]
fn test_extract_base_command() {
    assert_eq!(extract_base_command("cargo build"), "cargo");
    assert_eq!(extract_base_command("/usr/bin/cargo build"), "cargo");
    assert_eq!(extract_base_command("RUST_LOG=debug cargo test"), "cargo");
    assert_eq!(
        extract_base_command("FOO=1 BAR=2 /usr/local/bin/node script.js"),
        "node"
    );
    assert_eq!(extract_base_command("ls"), "ls");
    assert_eq!(extract_base_command(""), "");
}

#[test]
fn test_multiple_writable_paths() {
    // Use tempdir to avoid macOS /tmp -> /private/tmp symlink issues
    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    let config = SandboxConfig {
        enabled: true,
        allowed_commands: vec![],
        writable_paths: vec![
            "/home/user/project".into(),
            tmp_path.to_string_lossy().to_string(),
        ],
    };
    assert!(
        config
            .check_writable_path(Path::new("/home/user/project/src/main.rs"))
            .is_ok()
    );
    // Create a file inside the temp dir so canonicalize works
    let test_file = tmp_path.join("test.txt");
    std::fs::write(&test_file, "").unwrap();
    assert!(config.check_writable_path(&test_file).is_ok());
    assert!(
        config
            .check_writable_path(Path::new("/var/log/syslog"))
            .is_err()
    );
}

#[test]
fn test_error_messages_are_descriptive() {
    let config = SandboxConfig {
        enabled: true,
        allowed_commands: vec!["cargo".into()],
        writable_paths: vec!["/home/project".into()],
    };
    let cmd_err = config.check_command("wget http://evil.com").unwrap_err();
    assert!(cmd_err.contains("Sandbox"));
    assert!(cmd_err.contains("wget"));

    let path_err = config
        .check_writable_path(Path::new("/etc/shadow"))
        .unwrap_err();
    assert!(path_err.contains("Sandbox"));
    assert!(path_err.contains("/etc/shadow"));
}

#[test]
fn test_path_traversal_blocked() {
    let config = SandboxConfig {
        enabled: true,
        allowed_commands: vec![],
        writable_paths: vec!["/home/user/project".into()],
    };
    // Attempt to escape via `..`
    assert!(
        config
            .check_writable_path(Path::new("/home/user/project/../../etc/passwd"))
            .is_err()
    );
}
