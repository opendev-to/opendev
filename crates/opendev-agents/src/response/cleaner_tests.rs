use super::*;

#[test]
fn test_clean_none() {
    let cleaner = ResponseCleaner::new();
    assert!(cleaner.clean(None).is_none());
}

#[test]
fn test_clean_empty() {
    let cleaner = ResponseCleaner::new();
    assert!(cleaner.clean(Some("")).is_none());
}

#[test]
fn test_clean_normal_text() {
    let cleaner = ResponseCleaner::new();
    assert_eq!(
        cleaner.clean(Some("Hello, world!")),
        Some("Hello, world!".to_string())
    );
}

#[test]
fn test_clean_chat_template_tokens() {
    let cleaner = ResponseCleaner::new();
    let input = "Hello<|im_end|> world<|im_user|>";
    assert_eq!(cleaner.clean(Some(input)), Some("Hello world".to_string()));
}

#[test]
fn test_clean_tool_call_tags() {
    let cleaner = ResponseCleaner::new();
    let input = "<tool_call>some content</tool_call>";
    assert_eq!(cleaner.clean(Some(input)), Some("some content".to_string()));
}

#[test]
fn test_clean_tool_response_tags() {
    let cleaner = ResponseCleaner::new();
    let input = "<tool_response>result</tool_response>";
    assert_eq!(cleaner.clean(Some(input)), Some("result".to_string()));
}

#[test]
fn test_clean_function_tags() {
    let cleaner = ResponseCleaner::new();
    let input = "<function=read_file>args</function>";
    // <function=read_file> is removed, </function> is not matched by the patterns
    // but "args</function>" remains
    let result = cleaner.clean(Some(input));
    assert!(result.is_some());
    assert!(!result.as_ref().unwrap().contains("<function=read_file>"));
}

#[test]
fn test_clean_parameter_tags() {
    let cleaner = ResponseCleaner::new();
    let input = "<parameter name=\"path\">value</parameter>";
    assert_eq!(cleaner.clean(Some(input)), Some("value".to_string()));
}

#[test]
fn test_clean_mixed_tokens() {
    let cleaner = ResponseCleaner::new();
    let input = "Start <|im_start|>content<tool_call> end<|im_end|>";
    let result = cleaner.clean(Some(input)).unwrap();
    assert!(!result.contains("<|"));
    assert!(!result.contains("<tool_call>"));
    assert!(result.contains("Start"));
    assert!(result.contains("content"));
    assert!(result.contains("end"));
}

#[test]
fn test_clean_only_tokens_returns_none() {
    let cleaner = ResponseCleaner::new();
    let input = "<|im_end|><|im_start|>";
    assert!(cleaner.clean(Some(input)).is_none());
}

#[test]
fn test_clean_whitespace_trimming() {
    let cleaner = ResponseCleaner::new();
    let input = "  hello  <|im_end|>  ";
    assert_eq!(cleaner.clean(Some(input)), Some("hello".to_string()));
}

#[test]
fn test_clean_system_markers() {
    let cleaner = ResponseCleaner::new();
    let input = "Here is my response.\n[SYSTEM] You are repeating the same operation.\nContinuing with the task.";
    let result = cleaner.clean(Some(input)).unwrap();
    assert!(!result.contains("[SYSTEM]"));
    assert!(result.contains("Here is my response."));
    assert!(result.contains("Continuing with the task."));
}

#[test]
fn test_clean_internal_markers() {
    let cleaner = ResponseCleaner::new();
    let input = "Output text.\n[INTERNAL] debug diagnostic info\nMore output.";
    let result = cleaner.clean(Some(input)).unwrap();
    assert!(!result.contains("[INTERNAL]"));
    assert!(result.contains("Output text."));
    assert!(result.contains("More output."));
}

#[test]
fn test_clean_system_marker_only_at_line_start() {
    let cleaner = ResponseCleaner::new();
    // [SYSTEM] in the middle of a line should NOT be stripped
    let input = "The error said [SYSTEM] something";
    let result = cleaner.clean(Some(input)).unwrap();
    assert!(result.contains("[SYSTEM]"));
}
