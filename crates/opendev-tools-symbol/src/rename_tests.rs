use super::*;
use opendev_tools_lsp::protocol::{Position, SourceRange};

#[test]
fn test_handle_rename_missing_args() {
    let args = serde_json::json!({"symbol_name": "foo"});
    let result = handle_rename_symbol(&args, Path::new("/ws"));
    assert!(!result.success);
    assert!(result.output.contains("file_path"));
}

#[test]
fn test_handle_rename_invalid_identifier() {
    let args = serde_json::json!({
        "symbol_name": "foo",
        "file_path": "/tmp/test.rs",
        "new_name": "123invalid"
    });
    let result = handle_rename_symbol(&args, Path::new("/ws"));
    assert!(!result.success);
    assert!(result.output.contains("Invalid identifier"));
}

#[test]
fn test_apply_single_edit_single_line() {
    let mut lines = vec![
        "fn old_name() {".to_string(),
        "    println!(\"hello\");".to_string(),
        "}".to_string(),
    ];

    let edit = TextEdit::new(
        SourceRange::new(Position::new(0, 3), Position::new(0, 11)),
        "new_name",
    );

    apply_single_edit(&mut lines, &edit).unwrap();
    assert_eq!(lines[0], "fn new_name() {");
}

#[test]
fn test_apply_single_edit_multi_line() {
    let mut lines = vec![
        "line 0".to_string(),
        "line 1 start".to_string(),
        "line 2 middle".to_string(),
        "line 3 end rest".to_string(),
        "line 4".to_string(),
    ];

    let edit = TextEdit::new(
        SourceRange::new(Position::new(1, 7), Position::new(3, 10)),
        "REPLACED",
    );

    apply_single_edit(&mut lines, &edit).unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "line 0");
    assert_eq!(lines[1], "line 1 REPLACED rest");
    assert_eq!(lines[2], "line 4");
}

#[test]
fn test_apply_workspace_edit_empty() {
    let edit = WorkspaceEdit::new();
    let result = apply_workspace_edit(&edit, Path::new("/ws"));
    assert!(!result.success);
    assert!(result.output.contains("no changes"));
}

#[test]
fn test_apply_file_edits() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.rs");
    std::fs::write(&file, "fn old() {\n    old();\n}\n").unwrap();

    let edits = vec![
        TextEdit::new(
            SourceRange::new(Position::new(0, 3), Position::new(0, 6)),
            "new",
        ),
        TextEdit::new(
            SourceRange::new(Position::new(1, 4), Position::new(1, 7)),
            "new",
        ),
    ];

    let count = apply_file_edits(&file, &edits).unwrap();
    assert_eq!(count, 2);

    let content = std::fs::read_to_string(&file).unwrap();
    assert!(content.contains("fn new()"));
    assert!(content.contains("    new();"));
}
