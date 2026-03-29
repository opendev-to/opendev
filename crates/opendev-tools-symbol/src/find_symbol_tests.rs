use super::*;
use opendev_tools_lsp::protocol::{Position, SourceRange, SymbolKind, UnifiedSymbolInfo};
use std::path::PathBuf;

#[test]
fn test_handle_find_symbol_missing_name() {
    let args = serde_json::json!({});
    let result = handle_find_symbol(&args, Path::new("/workspace"));
    assert!(!result.success);
    assert!(result.output.contains("symbol_name"));
}

#[test]
fn test_handle_find_symbol_empty_name() {
    let args = serde_json::json!({"symbol_name": ""});
    let result = handle_find_symbol(&args, Path::new("/workspace"));
    assert!(!result.success);
}

#[test]
fn test_format_symbol_results_empty() {
    let result = format_symbol_results(&[], Path::new("/workspace"));
    assert!(result.success);
    assert!(result.output.contains("No symbols found"));
}

#[test]
fn test_format_symbol_results_with_symbols() {
    let symbols = vec![
        UnifiedSymbolInfo {
            name: "my_func".to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from("/workspace/src/main.rs"),
            range: SourceRange::new(Position::new(10, 0), Position::new(20, 1)),
            selection_range: None,
            container_name: Some("MyStruct".to_string()),
            detail: None,
        },
        UnifiedSymbolInfo {
            name: "MyClass".to_string(),
            kind: SymbolKind::Class,
            file_path: PathBuf::from("/workspace/src/models.py"),
            range: SourceRange::new(Position::new(5, 0), Position::new(50, 0)),
            selection_range: None,
            container_name: None,
            detail: None,
        },
    ];

    let result = format_symbol_results(&symbols, Path::new("/workspace"));
    assert!(result.success);
    assert!(result.output.contains("function"));
    assert!(result.output.contains("my_func"));
    assert!(result.output.contains("src/main.rs:11"));
    assert!(result.output.contains("class"));
    assert!(result.output.contains("MyClass"));
}

#[test]
fn test_body_preview_nonexistent_file() {
    let range = SourceRange::new(Position::new(0, 0), Position::new(5, 0));
    assert!(get_body_preview(Path::new("/nonexistent/file.rs"), &range).is_none());
}

#[test]
fn test_body_preview_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.rs");
    std::fs::write(&file, "line 0\nline 1\nline 2\nline 3\nline 4\n").unwrap();

    let range = SourceRange::new(Position::new(1, 0), Position::new(3, 0));
    let preview = get_body_preview(&file, &range).unwrap();
    assert!(preview.contains("line 1"));
    assert!(preview.contains("line 2"));
    assert!(preview.contains("line 3"));
}
