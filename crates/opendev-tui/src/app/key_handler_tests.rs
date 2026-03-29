use super::super::*;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn test_task_watcher_close_q() {
    let mut app = App::new();
    app.state.task_watcher_open = true;
    let key = crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    app.handle_key(key);
    assert!(!app.state.task_watcher_open, "q should close task watcher");
    assert!(app.state.force_clear, "q should set force_clear");
}

#[test]
fn test_task_watcher_close_esc() {
    let mut app = App::new();
    app.state.task_watcher_open = true;
    let key = crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    app.handle_key(key);
    assert!(
        !app.state.task_watcher_open,
        "Esc should close task watcher"
    );
    assert!(app.state.force_clear, "Esc should set force_clear");
}

#[test]
fn test_task_watcher_close_ctrl_b() {
    let mut app = App::new();
    app.state.task_watcher_open = true;
    let key = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    app.handle_key(key);
    assert!(
        !app.state.task_watcher_open,
        "Ctrl+B should close task watcher"
    );
    assert!(app.state.force_clear, "Ctrl+B should set force_clear");
}

#[test]
fn test_task_watcher_close_ctrl_p() {
    let mut app = App::new();
    app.state.task_watcher_open = true;
    let key = crossterm::event::KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
    app.handle_key(key);
    assert!(
        !app.state.task_watcher_open,
        "Ctrl+P should close task watcher"
    );
    assert!(app.state.force_clear, "Ctrl+P should set force_clear");
}

#[test]
fn test_task_watcher_close_alt_b() {
    let mut app = App::new();
    app.state.task_watcher_open = true;
    let key = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT);
    app.handle_key(key);
    assert!(
        !app.state.task_watcher_open,
        "Alt+B should close task watcher"
    );
    assert!(app.state.force_clear, "Alt+B should set force_clear");
}

#[test]
fn test_handle_key_char_input() {
    let mut app = App::new();
    let key = crossterm::event::KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    app.handle_key(key);
    assert_eq!(app.state.input_buffer, "a");
    assert_eq!(app.state.input_cursor, 1);
}

#[test]
fn test_handle_key_backspace() {
    let mut app = App::new();
    app.state.input_buffer = "abc".into();
    app.state.input_cursor = 3;
    let key = crossterm::event::KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
    app.handle_key(key);
    assert_eq!(app.state.input_buffer, "ab");
    assert_eq!(app.state.input_cursor, 2);
}

#[test]
fn test_handle_key_enter_submits() {
    let mut app = App::new();
    app.state.input_buffer = "hello".into();
    app.state.input_cursor = 5;
    let key = crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    app.handle_key(key);
    assert!(app.state.input_buffer.is_empty());
    assert_eq!(app.state.input_cursor, 0);
    // Should have added a user message
    assert_eq!(app.state.messages.len(), 1);
    assert_eq!(app.state.messages[0].role, DisplayRole::User);
}

#[test]
fn test_mode_toggle() {
    let mut app = App::new();
    let key = crossterm::event::KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
    app.handle_key(key);
    assert_eq!(app.state.mode, OperationMode::Plan);
    app.handle_key(key);
    assert_eq!(app.state.mode, OperationMode::Normal);
}

#[test]
fn test_page_scroll() {
    let mut app = App::new();
    let pgup = crossterm::event::KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE);
    app.handle_key(pgup);
    // First press: base=1 (no accel), page multiplier 3x = 3
    assert_eq!(app.state.scroll_offset, 3);
    assert!(app.state.user_scrolled);

    // Page down reduces offset; direction change resets accel, so base=1, 3x=3
    let pgdn = crossterm::event::KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
    app.handle_key(pgdn);
    assert_eq!(app.state.scroll_offset, 0);
    // user_scrolled only clears when already at 0 and page down again
    assert!(app.state.user_scrolled);

    // One more page down at 0 clears user_scrolled
    app.handle_key(pgdn);
    assert!(!app.state.user_scrolled);
}

#[test]
fn test_scroll_acceleration() {
    let mut app = App::new();
    // Set agent_active so Up/Down arrow scrolls (bypasses command history)
    app.state.agent_active = true;
    // First up-arrow: base amount = 1
    let up = crossterm::event::KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
    app.handle_key(up);
    assert_eq!(app.state.scroll_offset, 1);
    assert_eq!(app.state.scroll_accel_level, 0);

    // Immediate second press (within 200ms): accelerates to 2
    app.handle_key(up);
    assert_eq!(app.state.scroll_offset, 3); // 1 + 2
    assert_eq!(app.state.scroll_accel_level, 1);

    // Third press: accelerates to 3
    app.handle_key(up);
    assert_eq!(app.state.scroll_offset, 6); // 3 + 3
    assert_eq!(app.state.scroll_accel_level, 2);

    // Fourth press: stays at 3 (capped)
    app.handle_key(up);
    assert_eq!(app.state.scroll_offset, 9); // 6 + 3
    assert_eq!(app.state.scroll_accel_level, 2);
}

#[test]
fn test_scroll_acceleration_resets_on_direction_change() {
    let mut app = App::new();
    // Set agent_active so Up/Down arrow scrolls (bypasses command history)
    app.state.agent_active = true;
    let up = crossterm::event::KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
    let down = crossterm::event::KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);

    // Build up acceleration
    app.handle_key(up);
    app.handle_key(up);
    assert_eq!(app.state.scroll_accel_level, 1);
    assert_eq!(app.state.scroll_offset, 3); // 1 + 2

    // Direction change resets acceleration
    app.handle_key(down);
    assert_eq!(app.state.scroll_accel_level, 0);
    assert_eq!(app.state.scroll_offset, 2); // 3 - 1
}

#[test]
fn test_models_command_opens_picker_with_autocomplete() {
    let mut app = App::new();
    // Simulate typing "/models" character by character
    for c in "/models".chars() {
        let key = crossterm::event::KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        app.handle_key(key);
    }
    assert_eq!(app.state.input_buffer, "/models");

    // Autocomplete should be visible (showing /models command)
    // (It may or may not be visible depending on the completer setup in tests)

    // Press Enter — should execute /models, not accept autocomplete
    let enter = crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    app.handle_key(enter);

    // Input should be cleared (command was submitted)
    assert!(app.state.input_buffer.is_empty());

    // Should either open picker or show "No models" message
    let has_picker = app.model_picker_controller.is_some();
    let has_no_models_msg = app
        .state
        .messages
        .iter()
        .any(|m| m.content.contains("No models"));
    assert!(
        has_picker || has_no_models_msg,
        "Expected model picker or 'No models' message, got messages: {:?}",
        app.state
            .messages
            .iter()
            .map(|m| &m.content)
            .collect::<Vec<_>>()
    );
}
