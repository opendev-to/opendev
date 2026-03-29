use super::*;

#[test]
fn test_parse_sections_finds_templates() {
    let sections = SECTIONS.get_or_init(parse_sections);
    assert!(
        sections.contains_key("failed_tool_nudge"),
        "Should find failed_tool_nudge section"
    );
    assert!(
        sections.contains_key("nudge_permission_error"),
        "Should find nudge_permission_error section"
    );
    assert!(
        sections.contains_key("incomplete_todos_nudge"),
        "Should find incomplete_todos_nudge section"
    );
    assert!(
        sections.contains_key("consecutive_reads_nudge"),
        "Should find consecutive_reads_nudge section"
    );
    assert!(
        sections.contains_key("implicit_completion_nudge"),
        "Should find implicit_completion_nudge section"
    );
    assert!(
        sections.contains_key("all_todos_complete_nudge"),
        "Should find all_todos_complete_nudge section"
    );
    assert!(
        sections.contains_key("thinking_analysis_prompt"),
        "Should find thinking_analysis_prompt section"
    );
    assert!(
        sections.contains_key("thinking_analysis_prompt_with_todos"),
        "Should find thinking_analysis_prompt_with_todos section"
    );
    assert!(
        sections.contains_key("doom_loop_redirect_nudge"),
        "Should find doom_loop_redirect_nudge section"
    );
    assert!(
        sections.contains_key("doom_loop_stepback_nudge"),
        "Should find doom_loop_stepback_nudge section"
    );
    assert!(
        sections.contains_key("truncation_continue_directive"),
        "Should find truncation_continue_directive section"
    );
    assert!(
        sections.contains_key("doom_loop_compact_directive"),
        "Should find doom_loop_compact_directive section"
    );
    assert!(
        sections.contains_key("doom_loop_force_stop_message"),
        "Should find doom_loop_force_stop_message section"
    );
}

#[test]
fn test_get_reminder_with_vars() {
    let result = get_reminder(
        "incomplete_todos_nudge",
        &[("count", "3"), ("todo_list", "  - A\n  - B\n  - C")],
    );
    assert!(result.contains("3 incomplete todo(s)"));
    assert!(result.contains("  - A"));
}

#[test]
fn test_get_reminder_missing() {
    let result = get_reminder("nonexistent_section_xyz", &[]);
    assert!(result.is_empty());
}

#[test]
fn test_append_nudge() {
    let mut messages = Vec::new();
    append_nudge(&mut messages, "test nudge");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "[SYSTEM] test nudge");
    assert_eq!(messages[0]["_msg_class"], "nudge");
}

#[test]
fn test_inject_system_message_directive() {
    let mut messages = Vec::new();
    inject_system_message(&mut messages, "error context", MessageClass::Directive);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "[SYSTEM] error context");
    assert_eq!(messages[0]["_msg_class"], "directive");
}

#[test]
fn test_inject_system_message_internal() {
    let mut messages = Vec::new();
    inject_system_message(&mut messages, "debug info", MessageClass::Internal);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "[SYSTEM] debug info");
    assert_eq!(messages[0]["_msg_class"], "internal");
}

#[test]
fn test_append_directive() {
    let mut messages = Vec::new();
    append_directive(&mut messages, "strategy change");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["_msg_class"], "directive");
}

#[test]
fn test_message_class_as_str() {
    assert_eq!(MessageClass::Directive.as_str(), "directive");
    assert_eq!(MessageClass::Nudge.as_str(), "nudge");
    assert_eq!(MessageClass::Internal.as_str(), "internal");
}
