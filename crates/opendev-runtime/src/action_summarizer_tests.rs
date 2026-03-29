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
