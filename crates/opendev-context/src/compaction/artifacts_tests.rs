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
