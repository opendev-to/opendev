//! Glob matching supporting `*` and `**` patterns.
//!
//! Two modes are provided:
//! - [`glob_matches`]: permission mode where `*` matches any character including `/`.
//! - [`glob_matches_path`]: path mode where `*` does not match `/` but `**` does.

/// Glob matching for **tool permission patterns**.
///
/// `*` matches any characters including `/`, since tool arguments commonly contain paths.
/// The pattern is anchored: it must match the entire input string.
pub fn glob_matches(pattern: &str, input: &str) -> bool {
    glob_matches_impl(pattern.as_bytes(), input.as_bytes(), false)
}

/// Path-aware glob matching where `*` does NOT match `/` but `**` does.
///
/// Used for directory scope patterns like `src/**` or `vendor/*`.
pub fn glob_matches_path(pattern: &str, input: &str) -> bool {
    glob_matches_impl(pattern.as_bytes(), input.as_bytes(), true)
}

/// Core glob implementation.
///
/// When `slash_sensitive` is true, `*` does not match `/` (path mode).
/// When false, `*` matches any character (permission mode).
fn glob_matches_impl(pattern: &[u8], input: &[u8], slash_sensitive: bool) -> bool {
    let mut pi = 0;
    let mut ii = 0;
    let mut star_pi = usize::MAX;
    let mut star_ii = 0;
    // Track `**` separately since it always matches `/`
    let mut dstar_pi = usize::MAX;
    let mut dstar_ii = 0;

    while ii < input.len() {
        if pi + 1 < pattern.len() && pattern[pi] == b'*' && pattern[pi + 1] == b'*' {
            // `**` — matches everything including `/`
            dstar_pi = pi;
            dstar_ii = ii;
            pi += 2;
            // Skip trailing `/` after `**`
            if pi < pattern.len() && pattern[pi] == b'/' {
                pi += 1;
            }
            continue;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            // `*` — matches everything (or everything except `/` in path mode)
            star_pi = pi;
            star_ii = ii;
            pi += 1;
            continue;
        } else if pi < pattern.len() && (pattern[pi] == input[ii] || pattern[pi] == b'?') {
            pi += 1;
            ii += 1;
            continue;
        }

        // Backtrack to single `*`
        if star_pi != usize::MAX && (!slash_sensitive || input[star_ii] != b'/') {
            star_ii += 1;
            ii = star_ii;
            pi = star_pi + 1;
            continue;
        }

        // Backtrack to `**`
        if dstar_pi != usize::MAX {
            dstar_ii += 1;
            ii = dstar_ii;
            pi = dstar_pi + 2;
            if pi < pattern.len() && pattern[pi] == b'/' {
                pi += 1;
            }
            continue;
        }

        return false;
    }

    // Consume trailing `*` or `**`
    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

#[cfg(test)]
#[path = "glob_tests.rs"]
mod tests;
