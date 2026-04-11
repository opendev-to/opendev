//! Conditional rule evaluation for instruction files.
//!
//! Rules in `.opendev/rules/` can have YAML frontmatter with a `paths` field
//! containing glob patterns. These rules only apply when the agent is working
//! with files that match at least one of the patterns.

use std::path::{Path, PathBuf};

/// Check if a conditional rule applies to the given set of active file paths.
///
/// - `None` path_globs means the rule always applies (unconditional).
/// - Empty globs vec means the rule always applies.
/// - Otherwise, returns true if any active file matches any of the glob patterns.
///
/// Patterns are matched against relative paths from `working_dir`.
pub fn rule_applies(
    path_globs: Option<&[String]>,
    active_files: &[PathBuf],
    working_dir: &Path,
) -> bool {
    let globs = match path_globs {
        None | Some([]) => return true,
        Some(g) => g,
    };

    if active_files.is_empty() {
        return false;
    }

    let canon_wd = working_dir
        .canonicalize()
        .unwrap_or_else(|_| working_dir.to_path_buf());

    for file in active_files {
        let canon_file = file.canonicalize().unwrap_or_else(|_| file.clone());
        let relative = canon_file
            .strip_prefix(&canon_wd)
            .unwrap_or(&canon_file)
            .to_string_lossy();

        for glob_str in globs {
            if let Ok(pattern) = glob::Pattern::new(glob_str)
                && pattern.matches(relative.as_ref())
            {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
#[path = "conditional_tests.rs"]
mod tests;
