use super::*;

fn handler() -> FileHandler {
    FileHandler::new()
}

#[test]
fn test_handles_file_tools() {
    let h = handler();
    let handles = h.handles();
    assert!(handles.contains(&"Read"));
    assert!(handles.contains(&"Write"));
    assert!(handles.contains(&"Edit"));
    assert!(handles.contains(&"Glob"));
}

#[test]
fn test_read_tracking() {
    let h = handler();
    assert!(!h.was_read("/tmp/test.rs"));

    let mut args = HashMap::new();
    args.insert(
        "file_path".to_string(),
        Value::String("/tmp/test.rs".to_string()),
    );
    h.pre_check("Read", &args);

    assert!(h.was_read("/tmp/test.rs"));
}

#[test]
fn test_extract_changed_files_write() {
    let h = handler();
    let mut args = HashMap::new();
    args.insert(
        "file_path".to_string(),
        Value::String("/tmp/out.rs".to_string()),
    );
    let files = h.extract_changed_files("Write", &args);
    assert_eq!(files, vec!["/tmp/out.rs"]);
}

#[test]
fn test_extract_changed_files_read() {
    let h = handler();
    let mut args = HashMap::new();
    args.insert(
        "file_path".to_string(),
        Value::String("/tmp/in.rs".to_string()),
    );
    let files = h.extract_changed_files("Read", &args);
    assert!(files.is_empty());
}

#[test]
fn test_post_process_includes_changed_files() {
    let h = handler();
    let mut args = HashMap::new();
    args.insert(
        "file_path".to_string(),
        Value::String("/tmp/edited.rs".to_string()),
    );

    let result = h.post_process("Edit", &args, Some("ok"), None, true);
    assert!(result.success);
    assert_eq!(result.meta.changed_files, vec!["/tmp/edited.rs"]);
}

#[test]
fn test_pre_check_always_allows() {
    let h = handler();
    let args = HashMap::new();
    match h.pre_check("Write", &args) {
        PreCheckResult::Allow => {}
        other => panic!("Expected Allow, got {:?}", other),
    }
}
