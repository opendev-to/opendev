use super::*;

#[test]
fn test_range_contains() {
    let range = SourceRange::new(Position::new(5, 0), Position::new(10, 20));
    assert!(range.contains(Position::new(7, 10)));
    assert!(range.contains(Position::new(5, 0)));
    assert!(range.contains(Position::new(10, 20)));
    assert!(!range.contains(Position::new(4, 0)));
    assert!(!range.contains(Position::new(11, 0)));
}

#[test]
fn test_range_does_not_contain_before_start_char() {
    let range = SourceRange::new(Position::new(5, 3), Position::new(10, 20));
    assert!(!range.contains(Position::new(5, 2)));
    assert!(range.contains(Position::new(5, 3)));
}

#[test]
fn test_symbol_kind_from_lsp() {
    assert_eq!(SymbolKind::from_lsp(12), SymbolKind::Function);
    assert_eq!(SymbolKind::from_lsp(5), SymbolKind::Class);
    assert_eq!(SymbolKind::from_lsp(999), SymbolKind::Unknown);
}

#[test]
fn test_symbol_kind_display() {
    assert_eq!(SymbolKind::Function.display_name(), "function");
    assert_eq!(SymbolKind::Class.display_name(), "class");
}

#[test]
fn test_workspace_edit_counts() {
    let mut edit = WorkspaceEdit::new();
    edit.changes.insert(
        PathBuf::from("/a.rs"),
        vec![
            TextEdit::new(
                SourceRange::new(Position::new(0, 0), Position::new(0, 5)),
                "hello",
            ),
            TextEdit::new(
                SourceRange::new(Position::new(1, 0), Position::new(1, 3)),
                "world",
            ),
        ],
    );
    edit.changes.insert(
        PathBuf::from("/b.rs"),
        vec![TextEdit::new(
            SourceRange::new(Position::new(0, 0), Position::new(0, 1)),
            "x",
        )],
    );
    assert_eq!(edit.file_count(), 2);
    assert_eq!(edit.edit_count(), 3);
}

#[cfg(unix)]
#[test]
fn test_uri_path_roundtrip() {
    let path = PathBuf::from("/tmp/test.rs");
    let uri = path_to_uri_string(&path).unwrap();
    let back = uri_string_to_path(&uri).unwrap();
    assert_eq!(back, path);
}

#[test]
fn test_uri_string_to_path_non_file() {
    assert!(uri_string_to_path("http://example.com").is_none());
}

#[test]
fn test_text_edit_serde() {
    let edit = TextEdit::new(
        SourceRange::new(Position::new(1, 2), Position::new(3, 4)),
        "replacement",
    );
    let json = serde_json::to_string(&edit).unwrap();
    let back: TextEdit = serde_json::from_str(&json).unwrap();
    assert_eq!(back.new_text, "replacement");
    assert_eq!(back.range.start.line, 1);
}

#[test]
fn test_unified_symbol_info_serde() {
    let sym = UnifiedSymbolInfo {
        name: "my_func".to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from("/src/main.rs"),
        range: SourceRange::new(Position::new(10, 0), Position::new(20, 1)),
        selection_range: Some(SourceRange::new(
            Position::new(10, 4),
            Position::new(10, 11),
        )),
        container_name: Some("MyStruct".to_string()),
        detail: None,
    };
    let json = serde_json::to_string(&sym).unwrap();
    let back: UnifiedSymbolInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name, "my_func");
    assert_eq!(back.kind, SymbolKind::Function);
}

#[test]
fn test_parse_range_json() {
    let json = serde_json::json!({
        "start": { "line": 10, "character": 5 },
        "end": { "line": 20, "character": 15 }
    });
    let range = parse_range_json(&json).unwrap();
    assert_eq!(range.start.line, 10);
    assert_eq!(range.end.character, 15);
}

#[cfg(unix)]
#[test]
fn test_workspace_edit_from_json() {
    let json = serde_json::json!({
        "changes": {
            "file:///src/main.rs": [
                {
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 3 }
                    },
                    "newText": "let"
                }
            ]
        }
    });
    let edit = WorkspaceEdit::from_json(&json);
    assert_eq!(edit.file_count(), 1);
    assert_eq!(edit.edit_count(), 1);
}
