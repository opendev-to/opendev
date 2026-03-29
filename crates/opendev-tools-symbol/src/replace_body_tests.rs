use super::*;

#[test]
fn test_handle_replace_missing_args() {
    let args = serde_json::json!({"symbol_name": "foo"});
    let result = handle_replace_symbol_body(&args, Path::new("/ws"));
    assert!(!result.success);
    assert!(result.output.contains("file_path"));
}

#[test]
fn test_find_body_start_python_simple() {
    let lines = vec!["def my_func(x, y):", "    return x + y"];
    let result = find_body_start(&lines, 0, 0, LangCategory::Python);
    assert!(result.is_some());
    let (line, _col) = result.unwrap();
    assert_eq!(line, 1);
}

#[test]
fn test_find_body_start_python_with_docstring() {
    let lines = vec![
        "def my_func():",
        "    \"\"\"A docstring.\"\"\"",
        "    return 42",
    ];
    let result = find_body_start(&lines, 0, 0, LangCategory::Python);
    assert!(result.is_some());
    let (line, _) = result.unwrap();
    assert_eq!(line, 2);
}

#[test]
fn test_find_body_start_python_multiline_docstring() {
    let lines = vec![
        "def my_func():",
        "    \"\"\"",
        "    A multi-line docstring.",
        "    \"\"\"",
        "    return 42",
    ];
    let result = find_body_start(&lines, 0, 0, LangCategory::Python);
    assert!(result.is_some());
    let (line, _) = result.unwrap();
    assert_eq!(line, 4);
}

#[test]
fn test_find_body_start_c_like() {
    let lines = vec!["fn my_func() {", "    return 42;", "}"];
    let result = find_body_start(&lines, 0, 0, LangCategory::CLike);
    assert!(result.is_some());
    let (line, _) = result.unwrap();
    assert_eq!(line, 1);
}

#[test]
fn test_find_body_start_c_like_same_line() {
    let lines = vec!["fn f() { return 1; }"];
    let result = find_body_start(&lines, 0, 0, LangCategory::CLike);
    assert!(result.is_some());
    let (line, col) = result.unwrap();
    assert_eq!(line, 0);
    assert_eq!(col, 8);
}

#[test]
fn test_replace_range_whole_symbol() {
    let lines = vec!["line 0", "old body line 1", "old body line 2", "line 3"];
    let result = replace_range(&lines, 1, 0, 2, 15, "new body\n", false);
    assert!(result.contains("line 0"));
    assert!(result.contains("new body"));
    assert!(result.contains("line 3"));
    assert!(!result.contains("old body"));
}

#[test]
fn test_replace_range_preserve_signature() {
    let lines = vec!["def my_func():", "    old_body", "    more_old"];
    let result = replace_range(&lines, 1, 4, 2, 12, "    new_body", true);
    assert!(result.contains("def my_func():"));
    assert!(result.contains("    new_body"));
    assert!(!result.contains("old_body"));
}
