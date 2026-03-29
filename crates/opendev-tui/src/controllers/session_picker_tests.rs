use super::*;

fn sample_sessions() -> Vec<SessionOption> {
    vec![
        SessionOption {
            id: "abc123".into(),
            title: "Refactor auth module".into(),
            updated_at: "2026-03-19 10:00".into(),
            message_count: 12,
        },
        SessionOption {
            id: "def456".into(),
            title: "Fix login bug".into(),
            updated_at: "2026-03-18 15:30".into(),
            message_count: 5,
        },
        SessionOption {
            id: "ghi789".into(),
            title: "Add unit tests".into(),
            updated_at: "2026-03-17 09:00".into(),
            message_count: 20,
        },
    ]
}

#[test]
fn test_new_picker() {
    let picker = SessionPickerController::from_sessions(sample_sessions());
    assert!(picker.active());
    assert_eq!(picker.selected_index(), 0);
    assert_eq!(picker.filtered_count(), 3);
}

#[test]
fn test_new_empty() {
    let picker = SessionPickerController::new();
    assert!(picker.active());
    assert_eq!(picker.selected_index(), 0);
    assert_eq!(picker.filtered_count(), 0);
}

#[test]
fn test_next_wraps() {
    let mut picker = SessionPickerController::from_sessions(sample_sessions());
    picker.next();
    assert_eq!(picker.selected_index(), 1);
    picker.next();
    assert_eq!(picker.selected_index(), 2);
    picker.next();
    assert_eq!(picker.selected_index(), 0); // wrap
}

#[test]
fn test_prev_wraps() {
    let mut picker = SessionPickerController::from_sessions(sample_sessions());
    picker.prev();
    assert_eq!(picker.selected_index(), 2); // wrap back
    picker.prev();
    assert_eq!(picker.selected_index(), 1);
}

#[test]
fn test_select() {
    let mut picker = SessionPickerController::from_sessions(sample_sessions());
    picker.next(); // select index 1
    let selected = picker.select().unwrap();
    assert_eq!(selected.id, "def456");
    assert!(!picker.active());
}

#[test]
fn test_select_empty() {
    let mut picker = SessionPickerController::from_sessions(vec![]);
    assert!(picker.select().is_none());
}

#[test]
fn test_cancel() {
    let mut picker = SessionPickerController::from_sessions(sample_sessions());
    picker.cancel();
    assert!(!picker.active());
}

#[test]
fn test_search_filters_by_title() {
    let mut picker = SessionPickerController::from_sessions(sample_sessions());
    picker.search_push('l');
    picker.search_push('o');
    picker.search_push('g');
    picker.search_push('i');
    picker.search_push('n');
    assert_eq!(picker.filtered_count(), 1);
    let visible = picker.visible_sessions();
    assert_eq!(visible[0].1.id, "def456");
}

#[test]
fn test_search_filters_by_id() {
    let mut picker = SessionPickerController::from_sessions(sample_sessions());
    picker.search_push('g');
    picker.search_push('h');
    picker.search_push('i');
    assert_eq!(picker.filtered_count(), 1);
    let visible = picker.visible_sessions();
    assert_eq!(visible[0].1.id, "ghi789");
}

#[test]
fn test_search_pop_restores() {
    let mut picker = SessionPickerController::from_sessions(sample_sessions());
    picker.search_push('x');
    picker.search_push('y');
    picker.search_push('z');
    assert_eq!(picker.filtered_count(), 0);
    picker.search_pop();
    picker.search_pop();
    picker.search_pop();
    assert_eq!(picker.filtered_count(), 3);
}

#[test]
fn test_next_on_empty_is_noop() {
    let mut picker = SessionPickerController::from_sessions(vec![]);
    picker.next(); // should not panic
    assert_eq!(picker.selected_index(), 0);
}
