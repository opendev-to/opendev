//! Action summarizer — create concise spinner text from LLM responses.
//!
//! Two modes:
//! 1. **Heuristic** (default, zero-cost): Regex-based extraction of action phrases.
//! 2. **LLM-based** (optional): Uses a small model for high-quality summaries.

use std::borrow::Cow;

/// Maximum default summary length.
const DEFAULT_MAX_LENGTH: usize = 60;

/// Common action verbs to detect at the start of sentences.
const ACTION_VERBS: &[&str] = &[
    "reading",
    "writing",
    "editing",
    "searching",
    "analyzing",
    "creating",
    "modifying",
    "updating",
    "checking",
    "running",
    "building",
    "testing",
    "fixing",
    "implementing",
    "refactoring",
    "debugging",
    "deploying",
    "installing",
    "configuring",
    "deleting",
    "removing",
    "moving",
    "renaming",
    "copying",
    "downloading",
    "uploading",
    "parsing",
    "compiling",
    "formatting",
    "linting",
    "reviewing",
    "examining",
    "looking",
    "inspecting",
    "exploring",
    "scanning",
    "fetching",
    "loading",
    "saving",
    "committing",
    "pushing",
    "pulling",
    "merging",
    "rebasing",
    "cloning",
];

/// Prefixes that indicate the LLM is about to take an action.
const INTENT_PREFIXES: &[&str] = &[
    "I'll ",
    "I will ",
    "Let me ",
    "I need to ",
    "I'm going to ",
    "I am going to ",
    "Now I'll ",
    "Now let me ",
    "First, I'll ",
    "Next, I'll ",
];

/// Summarize an LLM response into a concise action phrase for spinner display.
///
/// Uses heuristics — no API call needed.
pub fn summarize_action(text: &str, max_length: usize) -> String {
    let max_len = if max_length == 0 {
        DEFAULT_MAX_LENGTH
    } else {
        max_length
    };

    // Try extracting from intent prefixes
    if let Some(summary) = extract_from_intent(text) {
        return truncate_to(&summary, max_len);
    }

    // Try finding a sentence starting with an action verb
    if let Some(summary) = extract_action_verb_sentence(text) {
        return truncate_to(&summary, max_len);
    }

    // Fallback: first sentence, cleaned up
    let first = first_sentence(text);
    truncate_to(&first, max_len)
}

/// Extract action from intent prefix ("I'll search the files" → "Searching the files").
fn extract_from_intent(text: &str) -> Option<String> {
    for prefix in INTENT_PREFIXES {
        if let Some(rest) = text.strip_prefix(prefix) {
            let sentence = first_clause(rest);
            if sentence.is_empty() {
                continue;
            }
            // Convert "search the files" → "Searching the files"
            let converted = verb_to_gerund(&sentence);
            return Some(capitalize_first(&converted));
        }
    }
    None
}

/// Find a sentence starting with an action verb in gerund form.
fn extract_action_verb_sentence(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for verb in ACTION_VERBS {
        if let Some(pos) = lower.find(verb) {
            // Only match at start of sentence (after newline, period, or start)
            if pos > 0 {
                let before = text.as_bytes()[pos - 1];
                if before != b'\n' && before != b'.' && before != b' ' {
                    continue;
                }
            }
            let rest = &text[pos..];
            let sentence = first_clause(rest);
            return Some(capitalize_first(&sentence));
        }
    }
    None
}

/// Convert base verb to gerund: "search" → "Searching", "read" → "Reading".
fn verb_to_gerund(text: &str) -> String {
    let words: Vec<&str> = text.splitn(2, char::is_whitespace).collect();
    if words.is_empty() {
        return text.to_string();
    }

    let verb = words[0].to_lowercase();
    let rest = if words.len() > 1 { words[1] } else { "" };

    // Check if already a gerund
    if verb.ends_with("ing") {
        return text.to_string();
    }

    let last_char = verb.chars().last();
    let second_last = verb.chars().nth(verb.len().saturating_sub(2));
    let third_last = verb.chars().nth(verb.len().saturating_sub(3));

    let gerund = if verb.ends_with('e') && !verb.ends_with("ee") {
        format!("{}ing", &verb[..verb.len() - 1])
    } else if verb.len() >= 3
        && last_char.is_some_and(is_consonant)
        && second_last.is_some_and(is_vowel)
        && third_last.is_some_and(is_consonant)
        && !verb.ends_with('w')
        && !verb.ends_with('x')
        && !verb.ends_with('y')
    {
        // Double the final consonant before adding -ing (e.g., "run" → "running")
        format!("{}{}", verb, last_char.unwrap_or_default()) + "ing"
    } else {
        format!("{verb}ing")
    };

    if rest.is_empty() {
        gerund
    } else {
        format!("{gerund} {rest}")
    }
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

fn is_consonant(c: char) -> bool {
    c.is_ascii_alphabetic() && !is_vowel(c)
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

fn first_sentence(text: &str) -> Cow<'_, str> {
    let line = text.lines().next().unwrap_or(text);
    if let Some(pos) = line.find(['.', '!', '?']) {
        Cow::Borrowed(&line[..pos])
    } else {
        Cow::Borrowed(line)
    }
}

fn first_clause(text: &str) -> String {
    let line = text.lines().next().unwrap_or(text);
    // End at period, comma, semicolon, or em-dash
    let end = line.find(['.', ';', '—']).unwrap_or(line.len());
    line[..end].trim().to_string()
}

fn truncate_to(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_intent_prefix() {
        let text = "I'll search through the configuration files to find the mode toggle";
        let summary = summarize_action(text, 60);
        assert!(summary.starts_with("Searching"));
        assert!(summary.len() <= 60);
    }

    #[test]
    fn test_summarize_let_me() {
        let text = "Let me read the file to understand the current implementation";
        let summary = summarize_action(text, 60);
        assert!(summary.starts_with("Reading"));
    }

    #[test]
    fn test_summarize_action_verb() {
        let text = "First, analyzing the code structure to identify components";
        let summary = summarize_action(text, 60);
        assert!(summary.contains("nalyzing"));
    }

    #[test]
    fn test_summarize_truncation() {
        let text = "I'll search through all the configuration files in the repository to find every instance of the mode toggle implementation across the entire codebase";
        let summary = summarize_action(text, 40);
        assert!(summary.len() <= 40);
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn test_verb_to_gerund() {
        assert_eq!(verb_to_gerund("search files"), "searching files");
        assert_eq!(verb_to_gerund("read the file"), "reading the file");
        assert_eq!(verb_to_gerund("write code"), "writing code");
        assert_eq!(verb_to_gerund("run tests"), "running tests");
        assert_eq!(verb_to_gerund("fix the bug"), "fixing the bug");
    }

    #[test]
    fn test_verb_already_gerund() {
        assert_eq!(verb_to_gerund("searching files"), "searching files");
    }

    #[test]
    fn test_capitalize_first() {
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("Already"), "Already");
    }

    #[test]
    fn test_first_sentence() {
        assert_eq!(
            first_sentence("Hello world. More text.").as_ref(),
            "Hello world"
        );
        assert_eq!(first_sentence("No period").as_ref(), "No period");
    }

    #[test]
    fn test_summarize_fallback() {
        let text = "The system needs attention";
        let summary = summarize_action(text, 60);
        assert_eq!(summary, "The system needs attention");
    }

    #[test]
    fn test_default_max_length() {
        let long = "I'll ".to_string() + &"do something very long ".repeat(10);
        let summary = summarize_action(&long, 0);
        assert!(summary.len() <= DEFAULT_MAX_LENGTH);
    }
}
