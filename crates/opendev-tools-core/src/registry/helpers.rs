//! Free-standing utility functions for tool name resolution and deduplication.

use std::collections::HashMap;

/// Simple Levenshtein edit distance between two strings.
pub(super) fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    #[allow(clippy::needless_range_loop)]
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = usize::from(a_chars[i - 1] != b_chars[j - 1]);
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

/// Convert a camelCase or PascalCase tool name to snake_case.
///
/// Examples: `ReadFile` -> `read_file`, `webFetch` -> `web_fetch`
pub(super) fn camel_to_snake_name(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }
    result
}

/// Create a dedup cache key from tool name and normalized args.
///
/// Uses a deterministic JSON serialization of sorted keys + tool name.
pub(super) fn make_dedup_key(tool_name: &str, args: &HashMap<String, serde_json::Value>) -> String {
    // Sort keys for deterministic hashing
    let mut sorted_args: Vec<(&String, &serde_json::Value)> = args.iter().collect();
    sorted_args.sort_by_key(|(k, _)| k.as_str());
    let args_str = serde_json::to_string(&sorted_args).unwrap_or_default();
    format!("{tool_name}:{args_str}")
}
