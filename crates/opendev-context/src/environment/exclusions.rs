//! Instruction file exclusion by glob patterns.
//!
//! Allows users to configure `instruction_excludes` in settings to skip
//! specific instruction files from being loaded. Managed instructions
//! (from /etc/opendev/) are never excludable.

use std::path::Path;

/// Check if an instruction file should be excluded based on glob patterns.
///
/// Managed instructions (paths under `/etc/opendev/`) are never excluded.
/// Each pattern is resolved relative to `working_dir` and matched against
/// the file's canonical path and its relative path from working_dir.
pub fn is_excluded(path: &Path, working_dir: &Path, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }

    // Managed instructions are never excludable
    let path_str = path.display().to_string();
    if path_str.starts_with("/etc/opendev/") {
        return false;
    }

    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let canonical_str = canonical.display().to_string();

    // Compute relative path from working_dir for pattern matching
    let canon_wd = working_dir
        .canonicalize()
        .unwrap_or_else(|_| working_dir.to_path_buf());
    let relative = canonical
        .strip_prefix(working_dir)
        .or_else(|_| canonical.strip_prefix(&canon_wd))
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| canonical.clone())
        .to_string_lossy()
        .to_string();

    for pattern in patterns {
        // Expand pattern relative to working_dir
        let expanded = if pattern.starts_with('/') || pattern.starts_with("~/") {
            pattern.clone()
        } else {
            working_dir.join(pattern).to_string_lossy().to_string()
        };

        // Try matching as a glob pattern against both canonical and relative paths
        if let Ok(glob_pattern) = glob::Pattern::new(&expanded)
            && (glob_pattern.matches(&canonical_str) || glob_pattern.matches(&relative))
        {
            return true;
        }

        // Also try the raw pattern against the relative path (for simple globs like "*.md")
        if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
            let filename = canonical.file_name().unwrap_or_default().to_string_lossy();
            if glob_pattern.matches(&relative) || glob_pattern.matches(filename.as_ref()) {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
#[path = "exclusions_tests.rs"]
mod tests;
