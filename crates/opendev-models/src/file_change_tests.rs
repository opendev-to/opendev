use super::*;

#[test]
fn test_file_change_type_serialization() {
    let ct = FileChangeType::Created;
    let json = serde_json::to_string(&ct).unwrap();
    assert_eq!(json, "\"created\"");

    let deserialized: FileChangeType = serde_json::from_str("\"modified\"").unwrap();
    assert_eq!(deserialized, FileChangeType::Modified);
}

#[test]
fn test_file_change_roundtrip() {
    let fc = FileChange::new(FileChangeType::Modified, "src/main.rs".to_string());
    let json = serde_json::to_string(&fc).unwrap();
    let deserialized: FileChange = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.file_path, "src/main.rs");
    assert_eq!(deserialized.change_type, FileChangeType::Modified);
}

#[test]
fn test_change_summary() {
    let fc = FileChange {
        lines_added: 10,
        lines_removed: 3,
        ..FileChange::new(FileChangeType::Modified, "test.rs".to_string())
    };
    assert_eq!(fc.get_change_summary(), "+10 -3");

    let created = FileChange::new(FileChangeType::Created, "new.rs".to_string());
    assert_eq!(created.get_change_summary(), "New file");
}

#[test]
fn test_file_icons() {
    assert_eq!(
        FileChange::new(FileChangeType::Created, "a".to_string()).get_file_icon(),
        "+"
    );
    assert_eq!(
        FileChange::new(FileChangeType::Deleted, "a".to_string()).get_file_icon(),
        "-"
    );
}
