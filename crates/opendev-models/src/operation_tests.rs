use super::*;

#[test]
fn test_operation_lifecycle() {
    let mut op = Operation::new(OperationType::FileWrite, "/tmp/test.rs".to_string());
    assert_eq!(op.status, OperationStatus::Pending);

    op.mark_executing();
    assert_eq!(op.status, OperationStatus::Executing);
    assert!(op.started_at.is_some());

    op.mark_success();
    assert_eq!(op.status, OperationStatus::Success);
    assert!(op.completed_at.is_some());
}

#[test]
fn test_operation_failure() {
    let mut op = Operation::new(OperationType::BashExecute, "ls".to_string());
    op.mark_executing();
    op.mark_failed("permission denied".to_string());
    assert_eq!(op.status, OperationStatus::Failed);
    assert_eq!(op.error.as_deref(), Some("permission denied"));
}

#[test]
fn test_write_result_roundtrip() {
    let result = WriteResult {
        success: true,
        file_path: "/tmp/test.rs".to_string(),
        created: true,
        size: 1024,
        error: None,
        operation_id: None,
        interrupted: false,
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: WriteResult = serde_json::from_str(&json).unwrap();
    assert!(deserialized.success);
    assert!(deserialized.created);
    assert_eq!(deserialized.size, 1024);
}

#[test]
fn test_bash_result_roundtrip() {
    let result = BashResult {
        success: true,
        command: "echo hello".to_string(),
        exit_code: 0,
        stdout: "hello\n".to_string(),
        stderr: String::new(),
        duration: 0.05,
        error: None,
        operation_id: None,
        background_task_id: None,
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: BashResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.exit_code, 0);
    assert_eq!(deserialized.stdout, "hello\n");
}
