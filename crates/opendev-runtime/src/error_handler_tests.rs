use super::*;

#[test]
fn test_error_action_from_char() {
    assert_eq!(ErrorAction::from_char('r'), Some(ErrorAction::Retry));
    assert_eq!(ErrorAction::from_char('s'), Some(ErrorAction::Skip));
    assert_eq!(ErrorAction::from_char('c'), Some(ErrorAction::Cancel));
    assert_eq!(ErrorAction::from_char('e'), Some(ErrorAction::Edit));
    assert_eq!(ErrorAction::from_char('x'), None);
}

#[test]
fn test_error_action_roundtrip() {
    for action in [
        ErrorAction::Retry,
        ErrorAction::Skip,
        ErrorAction::Cancel,
        ErrorAction::Edit,
    ] {
        assert_eq!(ErrorAction::from_char(action.as_char()), Some(action));
    }
}

#[test]
fn test_error_result_constructors() {
    let r = ErrorResult::retry();
    assert!(r.should_retry);
    assert!(!r.should_cancel);

    let s = ErrorResult::skip();
    assert!(!s.should_retry);
    assert!(!s.should_cancel);

    let c = ErrorResult::cancel();
    assert!(!c.should_retry);
    assert!(c.should_cancel);

    let e = ErrorResult::edit(serde_json::json!({"key": "value"}));
    assert!(e.should_retry);
    assert!(e.edited_params.is_some());
}

#[test]
fn test_available_actions_all() {
    let error = OperationError {
        message: "fail".into(),
        operation_type: "bash_execute".into(),
        target: "ls".into(),
        allow_retry: true,
        allow_edit: true,
    };
    let actions = available_actions(&error);
    assert_eq!(actions.len(), 4); // retry, edit, skip, cancel
}

#[test]
fn test_available_actions_no_retry_no_edit() {
    let error = OperationError {
        message: "fail".into(),
        operation_type: "file_write".into(),
        target: "/tmp/f".into(),
        allow_retry: false,
        allow_edit: false,
    };
    let actions = available_actions(&error);
    assert_eq!(actions.len(), 2); // skip, cancel
}

#[test]
fn test_resolve_choice_valid() {
    let error = OperationError {
        message: "fail".into(),
        operation_type: "bash_execute".into(),
        target: "ls".into(),
        allow_retry: true,
        allow_edit: false,
    };
    let result = resolve_choice('r', &error);
    assert!(result.is_some());
    assert!(result.unwrap().should_retry);

    let result = resolve_choice('s', &error);
    assert!(result.is_some());
    assert!(!result.unwrap().should_retry);
}

#[test]
fn test_resolve_choice_invalid() {
    let error = OperationError {
        message: "fail".into(),
        operation_type: "test".into(),
        target: "test".into(),
        allow_retry: false,
        allow_edit: false,
    };
    // retry not allowed
    assert!(resolve_choice('r', &error).is_none());
    // invalid char
    assert!(resolve_choice('x', &error).is_none());
}

#[test]
fn test_is_transient_error() {
    assert!(is_transient_error("Connection timeout"));
    assert!(is_transient_error("502 Bad Gateway"));
    assert!(is_transient_error("rate limit exceeded"));
    assert!(is_transient_error("Service Unavailable"));
    assert!(!is_transient_error("file not found"));
    assert!(!is_transient_error("permission denied"));
}

#[test]
fn test_error_action_serialize() {
    let json = serde_json::to_string(&ErrorAction::Retry).unwrap();
    assert_eq!(json, "\"retry\"");
    let deserialized: ErrorAction = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, ErrorAction::Retry);
}
