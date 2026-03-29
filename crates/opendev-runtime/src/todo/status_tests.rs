use super::*;

#[test]
fn test_todo_status_display() {
    assert_eq!(TodoStatus::Pending.to_string(), "todo");
    assert_eq!(TodoStatus::InProgress.to_string(), "doing");
    assert_eq!(TodoStatus::Completed.to_string(), "done");
}
