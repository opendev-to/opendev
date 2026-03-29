//! File-not-found suggestions using substring matching and Levenshtein distance.

/// Build an error message for a missing file, with up to 5 suggestions from the
/// parent directory using both substring matching and Levenshtein edit distance.
/// When the parent directory itself doesn't exist, lists available top-level directories.
pub(super) fn file_not_found_message(display_path: &str, resolved: &std::path::Path) -> String {
    let mut msg = format!("File not found: {display_path}");

    let basename = match resolved.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return msg,
    };
    let basename_lower = basename.to_lowercase();

    let parent = match resolved.parent() {
        Some(p) if p.is_dir() => p,
        Some(p) => {
            // Parent directory doesn't exist — find the nearest ancestor that does
            // and show available directories from there.
            let mut ancestor = p;
            while let Some(a) = ancestor.parent() {
                if a.is_dir() {
                    let available = crate::dir_hints::list_available_dirs(a);
                    if !available.is_empty() {
                        msg.push_str(&format!(
                            "\n\nNote: directory '{}' does not exist.\n\
                             Available directories in {}:\n{}",
                            p.display(),
                            a.display(),
                            available
                        ));
                    }
                    break;
                }
                ancestor = a;
            }
            return msg;
        }
        _ => return msg,
    };

    let entries = match std::fs::read_dir(parent) {
        Ok(rd) => rd,
        Err(_) => return msg,
    };

    // Collect candidates with a relevance score (lower is better).
    let mut scored: Vec<(String, usize)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let name_lower = name.to_lowercase();

        // Substring match gets score 0 (best)
        if name_lower.contains(&basename_lower) || basename_lower.contains(&name_lower) {
            scored.push((name, 0));
            continue;
        }

        // Levenshtein distance for typo detection
        let dist = levenshtein(&basename_lower, &name_lower);
        // Only suggest if edit distance is within 40% of the longer string length
        let max_dist = basename_lower.len().max(name_lower.len()) * 2 / 5;
        if dist <= max_dist.max(2) {
            scored.push((name, dist));
        }
    }

    if !scored.is_empty() {
        scored.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        scored.truncate(5);
        msg.push_str("\n\nDid you mean one of these?\n");
        for (s, _) in &scored {
            msg.push_str(&format!("  - {s}\n"));
        }
    }

    msg
}

/// Compute the Levenshtein edit distance between two strings.
pub(super) fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    // Use single-row optimization (O(n) space).
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

#[cfg(test)]
#[path = "suggestions_tests.rs"]
mod tests;
