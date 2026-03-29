use super::*;

#[test]
fn test_new_wrapper() {
    let wrapper = LspWrapper::new(None);
    assert!(wrapper.has_server_for(Path::new("test.rs")));
    assert!(wrapper.has_server_for(Path::new("test.py")));
    assert!(wrapper.has_server_for(Path::new("test.ts")));
    assert!(!wrapper.has_server_for(Path::new("test.unknown_ext")));
    assert!(!wrapper.has_server_for(Path::new("noext")));
}

#[test]
fn test_register_custom_server() {
    let mut wrapper = LspWrapper::new(None);
    assert!(!wrapper.has_server_for(Path::new("test.xyz")));

    wrapper.register_server(ServerConfig::new(
        "xyz-server",
        vec![],
        "xyz",
        vec!["xyz".to_string()],
    ));

    assert!(wrapper.has_server_for(Path::new("test.xyz")));
}

#[cfg(unix)]
#[test]
fn test_parse_symbol_info() {
    let json = serde_json::json!({
        "name": "my_function",
        "kind": 12,
        "location": {
            "uri": "file:///src/main.rs",
            "range": {
                "start": { "line": 5, "character": 0 },
                "end": { "line": 15, "character": 1 }
            }
        },
        "containerName": "MyStruct"
    });
    let sym = parse_symbol_info(&json).unwrap();
    assert_eq!(sym.name, "my_function");
    assert_eq!(sym.kind, protocol::SymbolKind::Function);
    assert_eq!(sym.container_name.as_deref(), Some("MyStruct"));
}

#[cfg(unix)]
#[test]
fn test_parse_locations() {
    let json = serde_json::json!([
        {
            "uri": "file:///a.rs",
            "range": {
                "start": { "line": 1, "character": 0 },
                "end": { "line": 1, "character": 10 }
            }
        },
        {
            "uri": "file:///b.rs",
            "range": {
                "start": { "line": 5, "character": 2 },
                "end": { "line": 5, "character": 8 }
            }
        }
    ]);
    let locs = parse_locations(&json);
    assert_eq!(locs.len(), 2);
    assert_eq!(locs[0].file_path, PathBuf::from("/a.rs"));
    assert_eq!(locs[1].range.start.line, 5);
}

#[test]
fn test_parse_empty_symbol_response() {
    let json = serde_json::json!([]);
    let syms = parse_symbol_response(&json);
    assert!(syms.is_empty());
}

#[test]
fn test_parse_null_symbol_response() {
    let json = serde_json::Value::Null;
    let syms = parse_symbol_response(&json);
    assert!(syms.is_empty());
}

#[test]
fn test_parse_document_symbol() {
    let json = serde_json::json!({
        "name": "MyClass",
        "kind": 5,
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 50, "character": 1 }
        },
        "selectionRange": {
            "start": { "line": 0, "character": 6 },
            "end": { "line": 0, "character": 13 }
        },
        "detail": "class",
        "children": [
            {
                "name": "my_method",
                "kind": 6,
                "range": {
                    "start": { "line": 5, "character": 4 },
                    "end": { "line": 10, "character": 5 }
                },
                "selectionRange": {
                    "start": { "line": 5, "character": 8 },
                    "end": { "line": 5, "character": 17 }
                }
            }
        ]
    });

    let result = serde_json::json!([json]);
    let syms = parse_document_symbols(&result, Path::new("/test.py"));
    // Should have child + parent = 2
    assert_eq!(syms.len(), 2);
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"MyClass"));
    assert!(names.contains(&"my_method"));
}
