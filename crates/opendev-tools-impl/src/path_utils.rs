//! Shared path resolution utilities for tool implementations.
//!
//! Path resolution functions (`expand_home`, `strip_curdir`, `normalize_path`,
//! `resolve_file_path`, `resolve_dir_path`) are defined in `opendev-tools-core::path`
//! and re-exported here for backward compatibility. This module additionally provides
//! security-boundary functions (`validate_path_access`, `is_sensitive_file`) that
//! are tool-level concerns.

use std::path::Path;

pub use opendev_tools_core::path::{
    expand_home, normalize_path, resolve_dir_path, resolve_file_path, strip_curdir,
};

/// Validate that a resolved path is safe to access.
///
/// Returns `Ok(())` if the path is within the working directory or an allowed
/// global config location. Returns `Err(message)` if the path would escape
/// the project boundary (e.g., via `../../../etc/passwd`).
///
/// Allowed paths outside working_dir:
/// - `~/.opendev/` (user config, memory, skills)
/// - `~/.config/opendev/` (XDG config)
/// - `/tmp/` (temporary files)
pub fn validate_path_access(resolved: &Path, working_dir: &Path) -> Result<(), String> {
    // Normalize the path: collapse `.` and `..` components logically.
    let normalized = normalize_path(resolved);

    // Check if it's under the working directory.
    if normalized.starts_with(working_dir) {
        return Ok(());
    }

    // Also accept if working_dir has symlinks — try canonical forms.
    if let (Ok(canon_path), Ok(canon_wd)) = (normalized.canonicalize(), working_dir.canonicalize())
        && canon_path.starts_with(&canon_wd)
    {
        return Ok(());
    }

    // Allow well-known global config directories.
    if let Some(home) = dirs::home_dir() {
        let allowed_prefixes = [
            home.join(".opendev"),
            home.join(".claude"),
            home.join(".config").join("opendev"),
        ];
        for prefix in &allowed_prefixes {
            if normalized.starts_with(prefix) {
                return Ok(());
            }
        }
    }

    // Allow /tmp for temporary files.
    if normalized.starts_with("/tmp") || normalized.starts_with("/var/tmp") {
        return Ok(());
    }

    Err(format!(
        "Access denied: path '{}' is outside the project directory '{}'",
        resolved.display(),
        working_dir.display()
    ))
}

/// Check if a file is likely to contain sensitive data (secrets, credentials, keys).
///
/// Matches patterns from `.gitignore` for Node.js (`.env` family) plus
/// common credential/key files. Returns a human-readable reason if sensitive.
pub fn is_sensitive_file(path: &Path) -> Option<&'static str> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    // .env files (matches .env, .env.local, .env.production, etc.)
    // but NOT .env.example or .env.sample
    if name == ".env"
        || (name.starts_with(".env.") && !name.ends_with(".example") && !name.ends_with(".sample"))
    {
        return Some("environment file (may contain secrets)");
    }

    // Private keys
    if name.ends_with(".pem")
        || name.ends_with(".key")
        || name == "id_rsa"
        || name == "id_ed25519"
        || name == "id_ecdsa"
    {
        return Some("private key file");
    }

    // Known credential files
    let credential_names = [
        "credentials",
        "credentials.json",
        "credentials.yaml",
        "credentials.yml",
        "service-account.json",
        ".npmrc",
        ".pypirc",
        ".netrc",
        ".htpasswd",
    ];
    if credential_names.contains(&name.as_str()) {
        return Some("credentials file");
    }

    // Token/secret files
    if name.contains("secret")
        && (name.ends_with(".json") || name.ends_with(".yaml") || name.ends_with(".yml"))
    {
        return Some("secrets file");
    }

    None
}

#[cfg(test)]
mod tests {
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
    fn test_validate_path_home_claude_allowed() {
        let tmp = TempDir::new().unwrap();
        let wd = tmp.path().canonicalize().unwrap();
        if let Some(home) = dirs::home_dir() {
            let claude_path = home.join(".claude/skills/my-skill.md");
            assert!(validate_path_access(&claude_path, &wd).is_ok());
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
}
