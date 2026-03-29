use super::*;

#[test]
fn test_handles() {
    let f = DirectoryFormatter;
    assert!(f.handles("Glob"));
    assert!(f.handles("Grep"));
    assert!(f.handles("list_files"));
    assert!(f.handles("search"));
    assert!(!f.handles("Bash"));
}

#[test]
fn test_format_glob() {
    let f = DirectoryFormatter;
    let output = "src/main.rs\nsrc/lib.rs\ntests/test.rs";
    let result = f.format("Glob", output);

    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("3 matching files"));
    assert_eq!(result.body.len(), 3);
    assert!(result.footer.is_none());
}

#[test]
fn test_format_grep() {
    let f = DirectoryFormatter;
    let output = "src/main.rs:10:fn main()\nsrc/lib.rs:5:pub mod foo";
    let result = f.format("Grep", output);

    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("2 matching results"));
}

#[test]
fn test_format_truncation() {
    let f = DirectoryFormatter;
    let lines: Vec<String> = (0..60).map(|i| format!("file_{i}.rs")).collect();
    let output = lines.join("\n");
    let result = f.format("Glob", &output);

    assert_eq!(result.body.len(), MAX_RESULTS);
    let footer = result.footer.unwrap();
    let footer_text: String = footer.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(footer_text.contains("20 more"));
}

#[test]
fn test_empty_lines_filtered() {
    let f = DirectoryFormatter;
    let output = "file1.rs\n\nfile2.rs\n\n";
    let result = f.format("Glob", output);

    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("2 matching files"));
}
