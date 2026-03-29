use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

// ---- Path validation tests ----

#[test]
fn test_validate_path_within_working_dir() {
    let tmp = TempDir::new().unwrap();
    let wd = tmp.path().canonicalize().unwrap();
    let file = wd.join("src/main.rs");
    assert!(validate_path_access(&file, &wd).is_ok());
}

#[test]
fn test_validate_path_traversal_blocked() {
    let tmp = TempDir::new().unwrap();
    let wd = tmp.path().canonicalize().unwrap();
    let escaped = wd.join("../../../etc/passwd");
    assert!(validate_path_access(&escaped, &wd).is_err());
}

#[test]
fn test_validate_path_absolute_outside_blocked() {
    let tmp = TempDir::new().unwrap();
    let wd = tmp.path().canonicalize().unwrap();
    let outside = Path::new("/etc/shadow");
    assert!(validate_path_access(outside, &wd).is_err());
}

#[test]
fn test_validate_path_tmp_allowed() {
    let tmp = TempDir::new().unwrap();
    let wd = tmp.path().canonicalize().unwrap();
    let tmp_file = Path::new("/tmp/opendev-test.txt");
    assert!(validate_path_access(tmp_file, &wd).is_ok());
}

#[test]
fn test_validate_path_home_opendev_allowed() {
    let tmp = TempDir::new().unwrap();
    let wd = tmp.path().canonicalize().unwrap();
    if let Some(home) = dirs::home_dir() {
        let config_path = home.join(".opendev/memory/test.md");
        assert!(validate_path_access(&config_path, &wd).is_ok());
    }
}

#[test]
fn test_validate_path_home_claude_blocked() {
    let tmp = TempDir::new().unwrap();
    let wd = tmp.path().canonicalize().unwrap();
    if let Some(home) = dirs::home_dir() {
        let claude_path = home.join(".claude/skills/my-skill.md");
        assert!(validate_path_access(&claude_path, &wd).is_err());
    }
}

#[test]
fn test_normalize_path_collapses_dotdot() {
    let result = normalize_path(Path::new("/home/user/project/../../../etc/passwd"));
    assert_eq!(result, PathBuf::from("/etc/passwd"));
}

#[test]
fn test_normalize_path_collapses_dot() {
    let result = normalize_path(Path::new("/home/user/./project/./src"));
    assert_eq!(result, PathBuf::from("/home/user/project/src"));
}

#[test]
fn test_normalize_path_preserves_root() {
    let result = normalize_path(Path::new("/../../etc"));
    assert_eq!(result, PathBuf::from("/etc"));
}

// ---- Sensitive file detection ----

#[test]
fn test_sensitive_env_file() {
    assert!(is_sensitive_file(Path::new(".env")).is_some());
    assert!(is_sensitive_file(Path::new("/project/.env")).is_some());
    assert!(is_sensitive_file(Path::new(".env.local")).is_some());
    assert!(is_sensitive_file(Path::new(".env.production")).is_some());
}

#[test]
fn test_sensitive_env_example_allowed() {
    assert!(is_sensitive_file(Path::new(".env.example")).is_none());
    assert!(is_sensitive_file(Path::new(".env.sample")).is_none());
}

#[test]
fn test_sensitive_private_keys() {
    assert!(is_sensitive_file(Path::new("server.pem")).is_some());
    assert!(is_sensitive_file(Path::new("private.key")).is_some());
    assert!(is_sensitive_file(Path::new("id_rsa")).is_some());
    assert!(is_sensitive_file(Path::new("id_ed25519")).is_some());
}

#[test]
fn test_sensitive_credentials() {
    assert!(is_sensitive_file(Path::new("credentials.json")).is_some());
    assert!(is_sensitive_file(Path::new(".npmrc")).is_some());
    assert!(is_sensitive_file(Path::new(".netrc")).is_some());
    assert!(is_sensitive_file(Path::new(".htpasswd")).is_some());
}

#[test]
fn test_sensitive_secrets_files() {
    assert!(is_sensitive_file(Path::new("app-secret.json")).is_some());
    assert!(is_sensitive_file(Path::new("secret.yaml")).is_some());
}

#[test]
fn test_non_sensitive_files() {
    assert!(is_sensitive_file(Path::new("main.rs")).is_none());
    assert!(is_sensitive_file(Path::new("README.md")).is_none());
    assert!(is_sensitive_file(Path::new("config.toml")).is_none());
    assert!(is_sensitive_file(Path::new("package.json")).is_none());
    assert!(is_sensitive_file(Path::new("Cargo.lock")).is_none());
}
