use super::*;

fn sample_options() -> Vec<String> {
    vec!["Rust".into(), "Python".into(), "Go".into()]
}

#[test]
fn test_new_is_inactive() {
    let ctrl = AskUserController::new();
    assert!(!ctrl.active());
}

#[test]
fn test_start_activates() {
    let mut ctrl = AskUserController::new();
    ctrl.start("Pick a language?".into(), sample_options(), None);
    assert!(ctrl.active());
    assert_eq!(ctrl.options().len(), 3);
    assert_eq!(ctrl.selected_index(), 0);
    assert!(ctrl.question().contains("language"));
}

#[test]
fn test_confirm_returns_selection() {
    let mut ctrl = AskUserController::new();
    ctrl.start("Pick?".into(), sample_options(), None);
    ctrl.next(); // index 1 = "Python"
    let answer = ctrl.confirm().unwrap();
    assert_eq!(answer, "Python");
    assert!(!ctrl.active());
}

#[test]
fn test_cancel_deactivates() {
    let mut ctrl = AskUserController::new();
    ctrl.start("Pick?".into(), sample_options(), Some("Go".into()));
    ctrl.cancel();
    assert!(!ctrl.active());
}

#[test]
fn test_default_value() {
    let mut ctrl = AskUserController::new();
    ctrl.start("Pick?".into(), sample_options(), Some("Go".into()));
    assert_eq!(ctrl.default_value(), Some("Go".into()));

    let mut ctrl2 = AskUserController::new();
    ctrl2.start("Pick?".into(), sample_options(), None);
    assert_eq!(ctrl2.default_value(), None);
}

#[test]
fn test_next_prev_wraps() {
    let mut ctrl = AskUserController::new();
    ctrl.start("Q?".into(), sample_options(), None);

    ctrl.next();
    assert_eq!(ctrl.selected_index(), 1);
    ctrl.next();
    ctrl.next();
    assert_eq!(ctrl.selected_index(), 0); // wrap

    ctrl.prev();
    assert_eq!(ctrl.selected_index(), 2); // wrap back
}

#[test]
fn test_confirm_empty_options_no_input() {
    let mut ctrl = AskUserController::new();
    ctrl.start("Q?".into(), vec![], None);
    // No text input and no default → None
    assert!(ctrl.confirm().is_none());
    assert!(ctrl.active()); // still active since nothing to confirm
}

#[test]
fn test_confirm_empty_options_with_default() {
    let mut ctrl = AskUserController::new();
    ctrl.start("Q?".into(), vec![], Some("yes".into()));
    // No text input but has default → returns default
    let answer = ctrl.confirm().unwrap();
    assert_eq!(answer, "yes");
    assert!(!ctrl.active());
}

#[test]
fn test_free_text_input() {
    let mut ctrl = AskUserController::new();
    ctrl.start("What's your name?".into(), vec![], None);
    assert!(!ctrl.has_options());

    ctrl.push_char('A');
    ctrl.push_char('l');
    ctrl.push_char('i');
    assert_eq!(ctrl.text_input(), "Ali");

    ctrl.pop_char();
    assert_eq!(ctrl.text_input(), "Al");

    ctrl.push_char('e');
    ctrl.push_char('x');
    let answer = ctrl.confirm().unwrap();
    assert_eq!(answer, "Alex");
    assert!(!ctrl.active());
}

#[test]
fn test_free_text_overrides_default() {
    let mut ctrl = AskUserController::new();
    ctrl.start("Name?".into(), vec![], Some("default".into()));
    ctrl.push_char('X');
    let answer = ctrl.confirm().unwrap();
    assert_eq!(answer, "X"); // typed text wins over default
}

#[test]
fn test_has_options() {
    let mut ctrl = AskUserController::new();
    ctrl.start("Q?".into(), sample_options(), None);
    assert!(ctrl.has_options());

    let mut ctrl2 = AskUserController::new();
    ctrl2.start("Q?".into(), vec![], None);
    assert!(!ctrl2.has_options());
}

#[test]
fn test_cleanup_clears_text_input() {
    let mut ctrl = AskUserController::new();
    ctrl.start("Q?".into(), vec![], None);
    ctrl.push_char('a');
    ctrl.push_char('b');
    ctrl.cancel();
    assert!(!ctrl.active());
    assert_eq!(ctrl.text_input(), "");
}
