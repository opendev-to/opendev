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
        let allowed_prefixes = [home.join(".opendev"), home.join(".config").join("opendev")];
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
#[path = "path_utils_tests.rs"]
mod tests;
