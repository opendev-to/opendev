use super::*;

#[test]
fn test_delta_operation_from_json() {
    let json = serde_json::json!({
        "type": "ADD",
        "section": "file_operations",
        "content": "Always read before write",
        "bullet_id": "fo-001",
        "metadata": {"helpful": 3}
    });
    let op = DeltaOperation::from_json(&json).unwrap();
    assert_eq!(op.op_type, DeltaOperationType::Add);
    assert_eq!(op.section, "file_operations");
    assert_eq!(op.content.as_deref(), Some("Always read before write"));
    assert_eq!(op.bullet_id.as_deref(), Some("fo-001"));
}

#[test]
fn test_delta_operation_tag_filters_metadata() {
    let json = serde_json::json!({
        "type": "TAG",
        "section": "testing",
        "bullet_id": "t-001",
        "metadata": {"helpful": 1, "invalid_key": 5, "harmful": 0}
    });
    let op = DeltaOperation::from_json(&json).unwrap();
    assert_eq!(op.metadata.len(), 2);
    assert_eq!(op.metadata.get("helpful"), Some(&1));
    assert_eq!(op.metadata.get("harmful"), Some(&0));
    assert!(!op.metadata.contains_key("invalid_key"));
}

#[test]
fn test_delta_operation_roundtrip() {
    let op = DeltaOperation {
        op_type: DeltaOperationType::Update,
        section: "code_nav".to_string(),
        content: Some("Search then read".to_string()),
        bullet_id: Some("cn-001".to_string()),
        metadata: HashMap::new(),
    };
    let json = op.to_json();
    let restored = DeltaOperation::from_json(&json).unwrap();
    assert_eq!(restored.op_type, DeltaOperationType::Update);
    assert_eq!(restored.content.as_deref(), Some("Search then read"));
}

#[test]
fn test_delta_operation_invalid_type() {
    let json = serde_json::json!({
        "type": "INVALID",
        "section": "x"
    });
    assert!(DeltaOperation::from_json(&json).is_none());
}

#[test]
fn test_delta_batch_from_json() {
    let json = serde_json::json!({
        "reasoning": "Updating playbook based on feedback",
        "operations": [
            {"type": "ADD", "section": "testing", "content": "Run tests after changes"},
            {"type": "TAG", "section": "nav", "bullet_id": "n-001", "metadata": {"helpful": 1}},
            {"type": "REMOVE", "section": "old", "bullet_id": "old-001"}
        ]
    });
    let batch = DeltaBatch::from_json(&json);
    assert_eq!(batch.reasoning, "Updating playbook based on feedback");
    assert_eq!(batch.operations.len(), 3);
    assert_eq!(batch.operations[0].op_type, DeltaOperationType::Add);
    assert_eq!(batch.operations[1].op_type, DeltaOperationType::Tag);
    assert_eq!(batch.operations[2].op_type, DeltaOperationType::Remove);
}

#[test]
fn test_delta_batch_roundtrip() {
    let batch = DeltaBatch {
        reasoning: "test reasoning".to_string(),
        operations: vec![DeltaOperation {
            op_type: DeltaOperationType::Add,
            section: "testing".to_string(),
            content: Some("Test content".to_string()),
            bullet_id: None,
            metadata: HashMap::new(),
        }],
    };
    let json = batch.to_json();
    let restored = DeltaBatch::from_json(&json);
    assert_eq!(restored.reasoning, "test reasoning");
    assert_eq!(restored.operations.len(), 1);
}

#[test]
fn test_delta_batch_empty_operations() {
    let json = serde_json::json!({"reasoning": "no ops"});
    let batch = DeltaBatch::from_json(&json);
    assert_eq!(batch.reasoning, "no ops");
    assert!(batch.operations.is_empty());
}

#[test]
fn test_operation_type_display() {
    assert_eq!(DeltaOperationType::Add.to_string(), "ADD");
    assert_eq!(DeltaOperationType::Update.to_string(), "UPDATE");
    assert_eq!(DeltaOperationType::Tag.to_string(), "TAG");
    assert_eq!(DeltaOperationType::Remove.to_string(), "REMOVE");
}

#[test]
fn test_operation_type_serde() {
    let json = serde_json::to_string(&DeltaOperationType::Add).unwrap();
    assert_eq!(json, r#""ADD""#);
    let deserialized: DeltaOperationType = serde_json::from_str(r#""TAG""#).unwrap();
    assert_eq!(deserialized, DeltaOperationType::Tag);
}
