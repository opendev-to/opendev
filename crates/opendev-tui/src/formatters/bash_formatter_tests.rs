use super::*;

#[test]
fn test_handles() {
    let f = BashFormatter;
    assert!(f.handles("Bash"));
    assert!(f.handles("run_command"));
    assert!(f.handles("bash_execute"));
    assert!(!f.handles("read_file"));
}

#[test]
fn test_format_success() {
    let f = BashFormatter;
    let output = "$ ls -la\nfile1.rs\nfile2.rs\nExit code: 0";
    let result = f.format("Bash", output);

    // Header should contain the command
    let header_text: String = result
        .header
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(header_text.contains("ls -la"));

    // Body should have 2 file lines
    assert_eq!(result.body.len(), 2);

    // Footer should show exit 0
    let footer = result.footer.unwrap();
    let footer_text: String = footer.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(footer_text.contains("0"));
}

#[test]
fn test_format_failure() {
    let f = BashFormatter;
    let output = "$ bad_command\ncommand not found\nExit code: 127";
    let result = f.format("Bash", output);

    let footer = result.footer.unwrap();
    // Check that exit code 127 is present and colored grey (same as success)
    let code_span = &footer.spans[1];
    assert_eq!(code_span.content.as_ref(), "127");
    assert_eq!(code_span.style.fg, Some(style_tokens::GREY));
}

#[test]
fn test_format_no_exit_code() {
    let f = BashFormatter;
    let output = "some output\nmore output";
    let result = f.format("Bash", output);
    assert!(result.footer.is_none());
    assert_eq!(result.body.len(), 2);
}

#[test]
fn test_parse_exit_code() {
    assert_eq!(
        BashFormatter::parse_exit_code("output\nExit code: 0"),
        Some(0)
    );
    assert_eq!(
        BashFormatter::parse_exit_code("output\nexit_code: 42"),
        Some(42)
    );
    assert_eq!(BashFormatter::parse_exit_code("no code here"), None);
}
