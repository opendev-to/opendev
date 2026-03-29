use super::*;

#[test]
fn test_no_truncation_short_output() {
    let sanitizer = ToolResultSanitizer::new();
    let result = sanitizer.sanitize("read_file", true, Some("short output"), None);
    assert!(!result.was_truncated);
    assert_eq!(result.output.as_deref(), Some("short output"));
}

#[test]
fn test_truncation_head_strategy() {
    let sanitizer = ToolResultSanitizer::new();
    let long_output = "x".repeat(20000);
    let result = sanitizer.sanitize("read_file", true, Some(&long_output), None);
    assert!(result.was_truncated);
    let output = result.output.unwrap();
    assert!(output.contains("[truncated:"));
    assert!(output.contains("strategy=head"));
}

#[test]
fn test_truncation_tail_strategy() {
    let sanitizer = ToolResultSanitizer::new();
    let long_output = "x".repeat(10000);
    let result = sanitizer.sanitize("run_command", true, Some(&long_output), None);
    assert!(result.was_truncated);
    let output = result.output.unwrap();
    assert!(output.contains("strategy=tail"));
}

#[test]
fn test_error_truncation() {
    let sanitizer = ToolResultSanitizer::new();
    let long_error = "e".repeat(5000);
    let result = sanitizer.sanitize("read_file", false, None, Some(&long_error));
    assert!(!result.was_truncated);
    let error = result.error.unwrap();
    assert!(error.len() <= ERROR_MAX_CHARS);
}

#[test]
fn test_error_not_truncated_when_short() {
    let sanitizer = ToolResultSanitizer::new();
    let result = sanitizer.sanitize("read_file", false, None, Some("file not found"));
    assert_eq!(result.error.as_deref(), Some("file not found"));
}

#[test]
fn test_no_rule_no_truncation() {
    let sanitizer = ToolResultSanitizer::new();
    let long_output = "x".repeat(50000);
    let result = sanitizer.sanitize("custom_tool", true, Some(&long_output), None);
    assert!(!result.was_truncated);
    assert_eq!(result.output.unwrap().len(), 50000);
}

#[test]
fn test_mcp_fallback() {
    let sanitizer = ToolResultSanitizer::new();
    let long_output = "x".repeat(10000);
    let result = sanitizer.sanitize_with_mcp_fallback(
        "mcp__github__list",
        true,
        Some(&long_output),
        None,
    );
    assert!(result.was_truncated);
}

#[test]
fn test_custom_limits() {
    let mut limits = HashMap::new();
    limits.insert("read_file".into(), 100);
    let sanitizer = ToolResultSanitizer::with_custom_limits(limits);

    let output = "x".repeat(200);
    let result = sanitizer.sanitize("read_file", true, Some(&output), None);
    assert!(result.was_truncated);
}

#[test]
fn test_empty_output() {
    let sanitizer = ToolResultSanitizer::new();
    let result = sanitizer.sanitize("read_file", true, Some(""), None);
    assert!(!result.was_truncated);
}

#[test]
fn test_none_output() {
    let sanitizer = ToolResultSanitizer::new();
    let result = sanitizer.sanitize("read_file", true, None, None);
    assert!(!result.was_truncated);
    assert!(result.output.is_none());
}

#[test]
fn test_truncate_head() {
    assert_eq!(truncate_head("hello world", 5), "hello");
}

#[test]
fn test_truncate_tail() {
    assert_eq!(truncate_tail("hello world", 5), "world");
}

#[test]
fn test_truncate_head_tail() {
    let text = "abcdefghij";
    let result = truncate_head_tail(text, 6, 0.5);
    assert!(result.starts_with("abc"));
    assert!(result.ends_with("hij"));
    assert!(result.contains("[middle truncated]"));
}

// ---- Overflow storage ----

#[test]
fn test_overflow_saved_on_truncation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let overflow_dir = tmp.path().join("tool-output");
    let sanitizer = ToolResultSanitizer::new().with_overflow_dir(overflow_dir.clone());

    let long_output = "x".repeat(20000);
    let result = sanitizer.sanitize("read_file", true, Some(&long_output), None);

    assert!(result.was_truncated);
    assert!(result.overflow_path.is_some());
    let path = result.overflow_path.unwrap();
    assert!(path.exists());

    // Full output should be in the file.
    let saved = std::fs::read_to_string(&path).unwrap();
    assert_eq!(saved.len(), 20000);

    // Truncated output should contain the hint.
    let output = result.output.unwrap();
    assert!(output.contains("Full output saved to:"));
    assert!(output.contains("read_file with offset/limit"));
}

#[test]
fn test_no_overflow_without_dir() {
    let sanitizer = ToolResultSanitizer::new();
    let long_output = "x".repeat(20000);
    let result = sanitizer.sanitize("read_file", true, Some(&long_output), None);
    assert!(result.was_truncated);
    assert!(result.overflow_path.is_none());
}

#[test]
fn test_no_overflow_when_not_truncated() {
    let tmp = tempfile::TempDir::new().unwrap();
    let overflow_dir = tmp.path().join("tool-output");
    let sanitizer = ToolResultSanitizer::new().with_overflow_dir(overflow_dir);

    let result = sanitizer.sanitize("read_file", true, Some("short"), None);
    assert!(!result.was_truncated);
    assert!(result.overflow_path.is_none());
}

#[test]
fn test_cleanup_overflow_removes_old_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let overflow_dir = tmp.path().join("tool-output");
    std::fs::create_dir_all(&overflow_dir).unwrap();

    // Create an "old" file with a timestamp 8 days ago.
    let old_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 8 * 24 * 60 * 60;
    let old_file = overflow_dir.join(format!("tool_{old_ts}_read_file.txt"));
    std::fs::write(&old_file, "old content").unwrap();

    // Create a "recent" file.
    let recent_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let recent_file = overflow_dir.join(format!("tool_{recent_ts}_search.txt"));
    std::fs::write(&recent_file, "recent content").unwrap();

    cleanup_overflow_dir(&overflow_dir);

    assert!(!old_file.exists(), "Old file should be removed");
    assert!(recent_file.exists(), "Recent file should be kept");
}

#[test]
fn test_overflow_file_capped_at_max_size() {
    let tmp = tempfile::TempDir::new().unwrap();
    let overflow_dir = tmp.path().join("tool-output");
    let sanitizer = ToolResultSanitizer::new().with_overflow_dir(overflow_dir);

    // Create output larger than MAX_OVERFLOW_BYTES (1 MB).
    let huge_output = "x".repeat(2 * 1024 * 1024);
    let result = sanitizer.sanitize("read_file", true, Some(&huge_output), None);

    assert!(result.was_truncated);
    let path = result.overflow_path.unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();

    // Saved file should be capped around MAX_OVERFLOW_BYTES, not the full 2 MB.
    assert!(
        saved.len() < MAX_OVERFLOW_BYTES + 200, // small margin for the omission marker
        "Overflow file should be capped: got {} bytes",
        saved.len()
    );
    assert!(
        saved.contains("bytes omitted from overflow file"),
        "Capped overflow should contain omission marker"
    );
}
