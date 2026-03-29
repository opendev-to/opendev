use super::*;

#[test]
fn test_handle_find_references_missing_name() {
    let args = serde_json::json!({});
    let result = handle_find_references(&args, Path::new("/ws"));
    assert!(!result.success);
    assert!(result.output.contains("symbol_name"));
}

#[test]
fn test_handle_find_references_missing_file() {
    let args = serde_json::json!({"symbol_name": "foo"});
    let result = handle_find_references(&args, Path::new("/ws"));
    assert!(!result.success);
    assert!(result.output.contains("file_path"));
}

#[test]
fn test_format_reference_results_empty() {
    let result = format_reference_results(&[], Path::new("/ws"));
    assert!(result.success);
    assert!(result.output.contains("No references found"));
}

#[test]
fn test_format_reference_results() {
    let refs = vec![
        SymbolReference {
            file: PathBuf::from("/ws/src/main.rs"),
            line: 10,
            character: 5,
        },
        SymbolReference {
            file: PathBuf::from("/ws/src/main.rs"),
            line: 20,
            character: 3,
        },
        SymbolReference {
            file: PathBuf::from("/ws/src/lib.rs"),
            line: 5,
            character: 0,
        },
    ];

    let result = format_reference_results(&refs, Path::new("/ws"));
    assert!(result.success);
    assert!(result.output.contains("3 reference(s)"));
    assert!(result.output.contains("2 file(s)"));
    assert!(result.output.contains("src/main.rs"));
    assert!(result.output.contains("src/lib.rs"));
}

#[test]
fn test_read_line_preview() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.rs");
    std::fs::write(&file, "line 0\nline 1\nline 2\n").unwrap();

    assert_eq!(read_line_preview(&file, 1).as_deref(), Some("line 1"));
    assert!(read_line_preview(&file, 100).is_none());
}
