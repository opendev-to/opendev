use super::*;

#[test]
fn test_format_thinking() {
    let content = "Let me analyze this step by step.";
    let formatted = ThinkingHandler::format_thinking(content);
    assert!(formatted.starts_with("--- thinking ---"));
    assert!(formatted.contains("step by step"));
    assert!(formatted.ends_with("--- end thinking ---"));
}

#[test]
fn test_format_thinking_empty() {
    assert!(ThinkingHandler::format_thinking("").is_empty());
}

#[test]
fn test_summarize_short() {
    let content = "brief thought";
    assert_eq!(ThinkingHandler::summarize(content, 10), "brief thought");
}

#[test]
fn test_summarize_long() {
    let content = "this is a very long thinking process that goes on and on";
    let summary = ThinkingHandler::summarize(content, 5);
    assert_eq!(summary, "this is a very long...");
}

#[test]
fn test_post_process_formats_output() {
    let h = ThinkingHandler::new();
    let args = HashMap::new();
    let result = h.post_process("Think", &args, Some("reasoning here"), None, true);
    assert!(result.output.unwrap().contains("--- thinking ---"));
}

#[test]
fn test_handles() {
    let h = ThinkingHandler::new();
    assert!(h.handles().contains(&"Think"));
    assert!(h.handles().contains(&"think"));
}
