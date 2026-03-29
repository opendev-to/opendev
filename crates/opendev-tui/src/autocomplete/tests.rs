use super::*;

#[test]
fn test_detect_trigger_slash() {
    let result = detect_trigger("/he");
    assert_eq!(result, Some((Trigger::Slash, "he".to_string())));
}

#[test]
fn test_detect_trigger_at() {
    let result = detect_trigger("hello @src/ma");
    assert_eq!(result, Some((Trigger::At, "src/ma".to_string())));
}

#[test]
fn test_detect_trigger_none() {
    let result = detect_trigger("hello world");
    assert_eq!(result, None);
}

#[test]
fn test_detect_trigger_slash_after_space() {
    let result = detect_trigger("some text /mo");
    assert_eq!(result, Some((Trigger::Slash, "mo".to_string())));
}

#[test]
fn test_detect_trigger_at_empty_query() {
    let result = detect_trigger("@");
    assert_eq!(result, Some((Trigger::At, String::new())));
}

#[test]
fn test_detect_trigger_slash_empty_query() {
    let result = detect_trigger("/");
    assert_eq!(result, Some((Trigger::Slash, String::new())));
}

#[test]
fn test_detect_trigger_mid_word_slash_ignored() {
    // `/` in the middle of a word without preceding whitespace
    let result = detect_trigger("path/to/file");
    assert_eq!(result, None);
}

#[test]
fn test_engine_command_completion() {
    let engine_dir = std::env::temp_dir();
    let mut engine = AutocompleteEngine::new(engine_dir);
    engine.update("/hel");
    assert!(engine.is_visible());
    assert!(!engine.items().is_empty());
    assert_eq!(engine.items()[0].kind, CompletionKind::Command);
    assert!(engine.items()[0].label.contains("help"));
}

#[test]
fn test_engine_dismiss() {
    let engine_dir = std::env::temp_dir();
    let mut engine = AutocompleteEngine::new(engine_dir);
    engine.update("/hel");
    assert!(engine.is_visible());
    engine.dismiss();
    assert!(!engine.is_visible());
    assert!(engine.items().is_empty());
}

#[test]
fn test_engine_select_navigation() {
    let engine_dir = std::env::temp_dir();
    let mut engine = AutocompleteEngine::new(engine_dir);
    // Complete with empty query to get all commands
    engine.update("/");
    assert!(engine.is_visible());
    let count = engine.items().len();
    assert!(count > 1);
    assert_eq!(engine.selected_index(), 0);

    engine.select_next();
    assert_eq!(engine.selected_index(), 1);

    engine.select_prev();
    assert_eq!(engine.selected_index(), 0);

    // Wrap around backwards
    engine.select_prev();
    assert_eq!(engine.selected_index(), count - 1);
}

#[test]
fn test_engine_accept() {
    let engine_dir = std::env::temp_dir();
    let mut engine = AutocompleteEngine::new(engine_dir);
    engine.update("/hel");
    assert!(engine.is_visible());
    let result = engine.accept();
    assert!(result.is_some());
    let (text, delete_count) = result.unwrap();
    assert_eq!(text, "/help");
    assert_eq!(delete_count, 4); // "/hel" = 4 chars
    assert!(!engine.is_visible());
}

#[test]
fn test_engine_accept_when_hidden() {
    let engine_dir = std::env::temp_dir();
    let mut engine = AutocompleteEngine::new(engine_dir);
    let result = engine.accept();
    assert!(result.is_none());
}

#[test]
fn test_detect_trigger_slash_arg() {
    let result = detect_trigger("/mode pl");
    assert_eq!(
        result,
        Some((
            Trigger::SlashArg {
                command: "mode".to_string()
            },
            "pl".to_string()
        ))
    );
}

#[test]
fn test_detect_trigger_slash_arg_empty() {
    let result = detect_trigger("/mode ");
    assert_eq!(
        result,
        Some((
            Trigger::SlashArg {
                command: "mode".to_string()
            },
            String::new()
        ))
    );
}

#[test]
fn test_engine_arg_completion_mode() {
    let engine_dir = std::env::temp_dir();
    let mut engine = AutocompleteEngine::new(engine_dir);
    engine.update("/mode pl");
    assert!(engine.is_visible());
    assert_eq!(engine.items().len(), 1);
    assert_eq!(engine.items()[0].label, "plan");
}

#[test]
fn test_engine_arg_completion_model_names() {
    let engine_dir = std::env::temp_dir();
    let mut engine = AutocompleteEngine::new(engine_dir);
    engine.update("/model gpt");
    assert!(engine.is_visible());
    assert!(engine.items().len() >= 2);
    for item in engine.items() {
        assert!(item.label.starts_with("gpt"));
    }
}

#[test]
fn test_engine_arg_completion_unknown_command() {
    let engine_dir = std::env::temp_dir();
    let mut engine = AutocompleteEngine::new(engine_dir);
    engine.update("/unknowncmd ");
    assert!(!engine.is_visible());
}
