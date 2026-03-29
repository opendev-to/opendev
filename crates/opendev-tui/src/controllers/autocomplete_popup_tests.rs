use super::*;

fn sample_items() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            text: "/help".into(),
            label: "/help".into(),
            description: Some("Show help".into()),
        },
        CompletionItem {
            text: "/mode".into(),
            label: "/mode".into(),
            description: Some("Switch mode".into()),
        },
        CompletionItem {
            text: "/models".into(),
            label: "/models".into(),
            description: None,
        },
    ]
}

#[test]
fn test_new_is_hidden() {
    let ctrl = AutocompletePopupController::new();
    assert!(!ctrl.visible());
    assert!(ctrl.items().is_empty());
}

#[test]
fn test_show_and_hide() {
    let mut ctrl = AutocompletePopupController::new();
    ctrl.show(sample_items());
    assert!(ctrl.visible());
    assert_eq!(ctrl.items().len(), 3);

    ctrl.hide();
    assert!(!ctrl.visible());
    assert!(ctrl.items().is_empty());
}

#[test]
fn test_show_empty_stays_hidden() {
    let mut ctrl = AutocompletePopupController::new();
    ctrl.show(vec![]);
    assert!(!ctrl.visible());
}

#[test]
fn test_navigation() {
    let mut ctrl = AutocompletePopupController::new();
    ctrl.show(sample_items());
    assert_eq!(ctrl.selected_index(), 0);

    ctrl.next();
    assert_eq!(ctrl.selected_index(), 1);
    ctrl.next();
    ctrl.next();
    assert_eq!(ctrl.selected_index(), 0); // wrap

    ctrl.prev();
    assert_eq!(ctrl.selected_index(), 2); // wrap back
}

#[test]
fn test_select() {
    let mut ctrl = AutocompletePopupController::new();
    ctrl.show(sample_items());
    ctrl.next(); // index 1
    let item = ctrl.select().unwrap();
    assert_eq!(item.text, "/mode");
    assert!(!ctrl.visible());
}

#[test]
fn test_select_when_hidden() {
    let mut ctrl = AutocompletePopupController::new();
    assert!(ctrl.select().is_none());
}
