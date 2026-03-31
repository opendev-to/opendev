use super::*;

#[test]
fn test_artifact_index() {
    let mut idx = ArtifactIndex::new();
    assert!(idx.is_empty());

    idx.record("src/main.rs", "created", "50 lines");
    assert_eq!(idx.len(), 1);

    idx.record("src/main.rs", "modified", "added tests");
    assert_eq!(idx.len(), 1); // Same file, updated in-place
    let entry = idx.entries.get("src/main.rs").unwrap();
    assert_eq!(entry.operation_count, 2);
    assert_eq!(entry.operations_seen, vec!["created", "modified"]);

    let summary = idx.as_summary();
    assert!(summary.contains("src/main.rs"));
    assert!(summary.contains("created, modified"));
}

#[test]
fn test_artifact_index_json_roundtrip() {
    let mut idx = ArtifactIndex::new();
    idx.record("src/main.rs", "created", "50 lines");
    idx.record("src/lib.rs", "modified", "added tests");

    let json = idx.to_json();
    let restored = ArtifactIndex::from_json(&json).unwrap();
    assert_eq!(restored.len(), 2);
    let entry = restored.entries.get("src/main.rs").unwrap();
    assert_eq!(entry.operation_count, 1);
    assert_eq!(entry.last_operation, "created");
}

#[test]
fn test_artifact_index_from_invalid_json() {
    let invalid = serde_json::json!("not an object");
    assert!(ArtifactIndex::from_json(&invalid).is_none());
}

#[test]
fn test_artifact_index_file_roundtrip() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("file-history.json");

    let mut idx = ArtifactIndex::new();
    idx.record("src/main.rs", "created", "50 lines");
    idx.record("src/lib.rs", "modified", "added tests");

    idx.save_to_file(&path).unwrap();
    let loaded = ArtifactIndex::load_from_file(&path).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(
        loaded.entries.get("src/main.rs").unwrap().last_operation,
        "created"
    );
}

#[test]
fn test_artifact_index_load_missing_file() {
    let result = ArtifactIndex::load_from_file(std::path::Path::new("/nonexistent/path.json"));
    assert!(result.is_none());
}

#[test]
fn test_artifact_index_merge_newer_wins() {
    let mut old = ArtifactIndex::new();
    old.record("file.rs", "created", "initial");

    // Simulate a newer entry
    let mut newer = ArtifactIndex::new();
    newer.record("file.rs", "modified", "updated");
    // Force a newer timestamp
    if let Some(entry) = newer.entries.get_mut("file.rs") {
        entry.updated_at = "9999-12-31T00:00:00+00:00".to_string();
    }

    old.merge(&newer);
    assert_eq!(
        old.entries.get("file.rs").unwrap().last_operation,
        "modified"
    );
}

#[test]
fn test_artifact_index_merge_caps_at_500() {
    let mut base = ArtifactIndex::new();
    for i in 0..510 {
        base.record(&format!("file_{i}.rs"), "created", "");
    }
    assert_eq!(base.len(), 510);

    let empty = ArtifactIndex::new();
    base.merge(&empty);
    assert!(base.len() <= 500);
}
