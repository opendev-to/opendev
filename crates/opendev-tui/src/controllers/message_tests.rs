use super::*;

#[test]
fn test_user_submit() {
    let controller = MessageController::new();
    let mut state = AppState::default();
    controller.handle_user_submit(&mut state, "hello");
    assert_eq!(state.messages.len(), 1);
    assert_eq!(state.messages[0].role, DisplayRole::User);
    assert_eq!(state.messages[0].content, "hello");
}

#[test]
fn test_agent_chunk_creates_message() {
    let controller = MessageController::new();
    let mut state = AppState::default();
    controller.handle_agent_chunk(&mut state, "Hello");
    assert_eq!(state.messages.len(), 1);
    assert_eq!(state.messages[0].role, DisplayRole::Assistant);
    assert_eq!(state.messages[0].content, "Hello");
}

#[test]
fn test_agent_chunk_appends() {
    let controller = MessageController::new();
    let mut state = AppState::default();
    controller.handle_agent_chunk(&mut state, "Hello");
    controller.handle_agent_chunk(&mut state, " world");
    assert_eq!(state.messages.len(), 1);
    assert_eq!(state.messages[0].content, "Hello world");
}

#[test]
fn test_agent_chunk_after_reasoning_creates_new_message() {
    let controller = MessageController::new();
    let mut state = AppState::default();
    // Simulate reasoning arriving first
    state
        .messages
        .push(DisplayMessage::new(DisplayRole::Reasoning, "thinking..."));
    // Then agent chunk arrives — should create a NEW assistant message
    controller.handle_agent_chunk(&mut state, "Hello");
    assert_eq!(state.messages.len(), 2);
    assert_eq!(state.messages[1].role, DisplayRole::Assistant);
    assert_eq!(state.messages[1].content, "Hello");
}

#[test]
fn test_agent_message() {
    use chrono::Utc;
    use std::collections::HashMap;

    let controller = MessageController::new();
    let mut state = AppState::default();
    let msg = ChatMessage {
        role: Role::Assistant,
        content: "Done.".into(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: Vec::new(),
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    };
    controller.handle_agent_message(&mut state, msg);
    assert_eq!(state.messages.len(), 1);
    assert_eq!(state.messages[0].role, DisplayRole::Assistant);
}
