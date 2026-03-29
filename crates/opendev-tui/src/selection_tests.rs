use super::*;

#[test]
fn test_text_position_ordering() {
    let a = TextPosition::new(0, 5);
    let b = TextPosition::new(0, 10);
    let c = TextPosition::new(1, 0);
    assert!(a < b);
    assert!(b < c);
    assert!(a < c);
}

#[test]
fn test_selection_range_ordered() {
    let range = SelectionRange {
        anchor: TextPosition::new(5, 10),
        cursor: TextPosition::new(2, 3),
    };
    let (start, end) = range.ordered();
    assert_eq!(start, TextPosition::new(2, 3));
    assert_eq!(end, TextPosition::new(5, 10));
}

#[test]
fn test_columns_on_line() {
    let range = SelectionRange {
        anchor: TextPosition::new(1, 5),
        cursor: TextPosition::new(3, 10),
    };
    // Before selection
    assert_eq!(range.columns_on_line(0, 80), None);
    // Start line
    assert_eq!(range.columns_on_line(1, 80), Some((5, 80)));
    // Middle line
    assert_eq!(range.columns_on_line(2, 80), Some((0, 80)));
    // End line
    assert_eq!(range.columns_on_line(3, 80), Some((0, 10)));
    // After selection
    assert_eq!(range.columns_on_line(4, 80), None);
}

#[test]
fn test_selection_state_lifecycle() {
    let mut state = SelectionState::default();
    state.conversation_area = Rect::new(0, 0, 80, 24);
    state.actual_scroll = 0;

    state.start(10, 5);
    assert!(state.active);
    assert!(state.range.is_some());

    state.extend(20, 10);
    let range = state.range.unwrap();
    assert_eq!(range.cursor.line_index, 10);

    let has_selection = state.finalize();
    assert!(has_selection);
    assert!(!state.active);
}

#[test]
fn test_single_click_no_selection() {
    let mut state = SelectionState::default();
    state.conversation_area = Rect::new(0, 0, 80, 24);
    state.actual_scroll = 0;

    state.start(10, 5);
    // No extend — mouse up at same position
    let has_selection = state.finalize();
    assert!(!has_selection);
}
