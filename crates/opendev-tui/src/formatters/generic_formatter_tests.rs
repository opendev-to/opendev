use super::*;

#[test]
fn test_handles_anything() {
    let f = GenericFormatter;
    assert!(f.handles("anything"));
    assert!(f.handles("random_tool"));
    assert!(f.handles(""));
}

#[test]
fn test_format_plain_text() {
    let f = GenericFormatter;
    let result = f.format("some_tool", "hello world\nsecond line");

    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("some_tool"));
    assert_eq!(result.body.len(), 2);
    assert!(result.footer.is_none());
}

#[test]
fn test_format_json_pretty() {
    let f = GenericFormatter;
    let json = r#"{"key":"value","nested":{"a":1}}"#;
    let result = f.format("api_call", json);

    // Body should have multiple lines (pretty-printed)
    assert!(result.body.len() > 1);

    // Check that body contains the key
    let body_text: String = result
        .body
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(body_text.contains("key"));
    assert!(body_text.contains("value"));
}

#[test]
fn test_format_invalid_json_fallback() {
    let f = GenericFormatter;
    let output = "not json {broken";
    let result = f.format("tool", output);
    assert_eq!(result.body.len(), 1);
}

#[test]
fn test_format_truncation() {
    let f = GenericFormatter;
    let lines: Vec<String> = (0..100).map(|i| format!("line {i}")).collect();
    let output = lines.join("\n");
    let result = f.format("tool", &output);

    assert!(result.footer.is_some());
}
