use super::*;

#[test]
fn test_dispatch_bash() {
    let result = FormatterFactory::format("Bash", "$ echo hi\nhi\nExit code: 0");
    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("echo hi"));
}

#[test]
fn test_dispatch_read() {
    let result = FormatterFactory::format("Read", "line 1\nline 2");
    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("2 lines"));
}

#[test]
fn test_dispatch_write() {
    let result = FormatterFactory::format("Write", "+new");
    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("Written"));
}

#[test]
fn test_dispatch_edit() {
    let result = FormatterFactory::format("Edit", "-old\n+new");
    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("Edited"));
}

#[test]
fn test_dispatch_glob() {
    let result = FormatterFactory::format("Glob", "a.rs\nb.rs");
    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("2 matching files"));
}

#[test]
fn test_dispatch_grep() {
    let result = FormatterFactory::format("Grep", "file:1:match");
    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("1 matching results"));
}

#[test]
fn test_dispatch_unknown_falls_to_generic() {
    let result = FormatterFactory::format("unknown_tool", "some output");
    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("unknown_tool"));
}
