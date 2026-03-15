//! Shared path resolution utilities for tool implementations.
//!
//! LLMs sometimes redundantly specify the project directory name as a path
//! parameter when the working directory is already that project. For example,
//! if the working dir is `/home/user/myproject` and the LLM passes `path: "myproject"`,
//! the naïve join produces `/home/user/myproject/myproject` which doesn't exist.
//!
//! These utilities detect and correct such cases.

use std::path::{Path, PathBuf};

/// Expand tilde (`~`) and `$HOME` prefixes in a path string.
///
/// - `~/foo` → `/home/user/foo`
/// - `$HOME/foo` → `/home/user/foo`
/// - `~` → `/home/user`
/// - Other paths are returned as-is.
pub fn expand_home(path: &str) -> String {
    if path == "~" {
        return dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return format!("{}/{}", home.display(), rest);
    }
    if let Some(rest) = path.strip_prefix("$HOME/")
        && let Some(home) = dirs::home_dir()
    {
        return format!("{}/{}", home.display(), rest);
    }
    if path == "$HOME" {
        return dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
    }
    path.to_string()
}

/// Resolve a user-provided directory path against the working directory.
///
/// Handles the common LLM mistake where the path is the same as the working
/// directory's basename (e.g., passing `"myproject"` when cwd is already
/// `/home/user/myproject`), which would otherwise produce a doubled path.
pub fn resolve_dir_path(user_path: &str, working_dir: &Path) -> PathBuf {
    let path = Path::new(user_path);
    if path.is_absolute() {
        if path.is_dir() {
            path.to_path_buf()
        } else {
            // Absolute path doesn't exist as a directory — check if it matches
            // the working directory or is a parent prefix of it.
            if working_dir.starts_with(path) || working_dir == path {
                working_dir.to_path_buf()
            } else {
                path.to_path_buf()
            }
        }
    } else {
        let joined = working_dir.join(path);
        if joined.is_dir() {
            joined
        } else if working_dir
            .file_name()
            .map(|n| n == user_path)
            .unwrap_or(false)
        {
            // The relative path is the same as the working dir's basename,
            // meaning the LLM redundantly specified it. Fall back to cwd.
            working_dir.to_path_buf()
        } else {
            joined
        }
    }
}

/// Resolve a user-provided file path against the working directory.
///
/// Similar to [`resolve_dir_path`] but for file paths. If a relative path
/// doesn't exist when joined with working_dir, checks if stripping a redundant
/// leading directory component (matching the working dir's basename) helps.
pub fn resolve_file_path(user_path: &str, working_dir: &Path) -> PathBuf {
    let path = Path::new(user_path);
    if path.is_absolute() {
        if path.exists() {
            return path.to_path_buf();
        }
        // Check if the path has a redundant component matching the working dir basename.
        // e.g., /home/user/myproject/myproject/src/main.rs -> /home/user/myproject/src/main.rs
        if let Ok(rel) = path.strip_prefix(working_dir)
            && let Some(first) = rel.components().next()
        {
            let first_name = first.as_os_str();
            if working_dir
                .file_name()
                .map(|n| n == first_name)
                .unwrap_or(false)
            {
                let fixed = working_dir.join(rel.strip_prefix(first_name).unwrap_or(rel));
                // Accept if the file exists OR its parent directory exists
                // (supports new file creation with redundant prefix)
                if fixed.exists() || fixed.parent().map(|p| p.is_dir()).unwrap_or(false) {
                    return fixed;
                }
            }
        }
        path.to_path_buf()
    } else {
        let joined = working_dir.join(path);
        if joined.exists() {
            return joined;
        }
        // Check if first component matches working dir basename (redundant prefix)
        let mut components = path.components();
        if let Some(first) = components.next() {
            let first_name = first.as_os_str();
            if working_dir
                .file_name()
                .map(|n| n == first_name)
                .unwrap_or(false)
            {
                let rest: PathBuf = components.collect();
                if !rest.as_os_str().is_empty() {
                    let fixed = working_dir.join(&rest);
                    if fixed.exists() || fixed.parent().map(|p| p.is_dir()).unwrap_or(false) {
                        return fixed;
                    }
                }
            }
        }
        joined
    }
}

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

/// Normalize a path by collapsing `.` and `..` components without touching the filesystem.
///
/// Unlike `canonicalize()`, this works on paths that don't exist yet.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::CurDir => {} // skip `.`
            Component::ParentDir => {
                // Pop the last component if it's a normal component.
                if let Some(last) = components.last()
                    && !matches!(last, Component::RootDir | Component::Prefix(_))
                {
                    components.pop();
                    continue;
                }
                components.push(component);
            }
            _ => components.push(component),
        }
    }

    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_dir_path_redundant_basename() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();

        // LLM passes "myproject" when working dir is already myproject
        let result = resolve_dir_path("myproject", &project);
        assert_eq!(result, project);
    }

    #[test]
    fn test_resolve_dir_path_valid_subdir() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        let subdir = project.join("src");
        fs::create_dir_all(&subdir).unwrap();

        // LLM passes "src" which is a valid subdirectory
        let result = resolve_dir_path("src", &project);
        assert_eq!(result, subdir);
    }

    #[test]
    fn test_resolve_dir_path_absolute_existing() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("existing");
        fs::create_dir(&dir).unwrap();

        let result = resolve_dir_path(dir.to_str().unwrap(), tmp.path());
        assert_eq!(result, dir);
    }

    #[test]
    fn test_resolve_dir_path_no_path() {
        let tmp = TempDir::new().unwrap();
        // When no path is provided, caller uses working_dir directly (not this fn)
        // But test that a non-existent relative path still returns the join
        let result = resolve_dir_path("nonexistent", tmp.path());
        assert_eq!(result, tmp.path().join("nonexistent"));
    }

    #[test]
    fn test_resolve_file_path_redundant_prefix() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("main.rs"), "fn main() {}").unwrap();

        // LLM passes "myproject/main.rs" when working dir is already myproject
        let result = resolve_file_path("myproject/main.rs", &project);
        assert_eq!(result, project.join("main.rs"));
    }

    #[test]
    fn test_resolve_file_path_valid_relative() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        let src = project.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.rs"), "").unwrap();

        let result = resolve_file_path("src/lib.rs", &project);
        assert_eq!(result, project.join("src/lib.rs"));
    }

    #[test]
    fn test_resolve_file_path_absolute_redundant() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("main.rs"), "").unwrap();

        // Absolute path with doubled project name
        let wrong_path = project.join("myproject").join("main.rs");
        let result = resolve_file_path(wrong_path.to_str().unwrap(), &project);
        assert_eq!(result, project.join("main.rs"));
    }

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
        // Attempt to escape via ..
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
        // Can't go above root
        assert_eq!(result, PathBuf::from("/etc"));
    }

    // ---- expand_home ----

    #[test]
    fn test_expand_home_tilde_prefix() {
        let result = expand_home("~/projects/foo");
        assert!(!result.starts_with("~"));
        assert!(result.ends_with("/projects/foo"));
    }

    #[test]
    fn test_expand_home_tilde_only() {
        let result = expand_home("~");
        assert!(!result.starts_with("~"));
        // Should be a real absolute path
        assert!(result.starts_with('/'));
    }

    #[test]
    fn test_expand_home_dollar_home_prefix() {
        let result = expand_home("$HOME/Documents");
        assert!(!result.starts_with("$HOME"));
        assert!(result.ends_with("/Documents"));
    }

    #[test]
    fn test_expand_home_dollar_home_only() {
        let result = expand_home("$HOME");
        assert!(!result.contains("$HOME"));
        assert!(result.starts_with('/'));
    }

    #[test]
    fn test_expand_home_no_expansion() {
        assert_eq!(expand_home("/absolute/path"), "/absolute/path");
        assert_eq!(expand_home("relative/path"), "relative/path");
        assert_eq!(expand_home("~not-a-tilde"), "~not-a-tilde");
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
