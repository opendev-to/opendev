use super::*;

#[test]
fn test_handles() {
    let f = FileFormatter;
    assert!(f.handles("Read"));
    assert!(f.handles("Write"));
    assert!(f.handles("Edit"));
    assert!(f.handles("read_file"));
    assert!(f.handles("write_file"));
    assert!(f.handles("edit_file"));
    assert!(!f.handles("Bash"));
}

#[test]
fn test_format_read() {
    let f = FileFormatter;
    let output = "fn main() {\n    println!(\"hello\");\n}";
    let result = f.format("Read", output);

    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("3 lines"));
    assert_eq!(result.body.len(), 3);
}

#[test]
fn test_format_edit_diff() {
    let f = FileFormatter;
    let output = " context line\n-old line\n+new line\n context again";
    let result = f.format("Edit", output);

    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("Edited"));

    // Check footer has +/- counts
    let footer = result.footer.unwrap();
    let footer_text: String = footer.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(footer_text.contains("+1"));
    assert!(footer_text.contains("-1"));
}

#[test]
fn test_format_write() {
    let f = FileFormatter;
    let result = f.format("Write", "+new content");

    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("Written"));
}

#[test]
fn test_format_read_truncation() {
    let f = FileFormatter;
    let lines: Vec<String> = (0..100).map(|i| format!("line {i}")).collect();
    let output = lines.join("\n");
    let result = f.format("Read", &output);

    // Should have a footer indicating truncation
    assert!(result.footer.is_some());
}
