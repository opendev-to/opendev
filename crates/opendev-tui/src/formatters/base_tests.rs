use super::*;

#[test]
fn test_truncate_lines_short() {
    let text = "line 1\nline 2\nline 3";
    assert_eq!(truncate_lines(text, 10), text);
}

#[test]
fn test_truncate_lines_long() {
    let lines: Vec<String> = (0..100).map(|i| format!("line {i}")).collect();
    let text = lines.join("\n");
    let result = truncate_lines(&text, 20);
    assert!(result.contains("omitted"));
    assert!(result.lines().count() <= 21);
}

#[test]
fn test_indent() {
    let text = "line 1\nline 2\n\nline 3";
    let result = indent(text, 4);
    assert!(result.starts_with("    line 1"));
    assert!(result.contains("\n    line 2"));
    // Empty lines stay empty
    assert!(result.contains("\n\n"));
}
