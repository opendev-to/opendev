//! Message handling controller.
//!
//! Coordinates between user input, agent responses, and the conversation display.
//! Handles system reminder filtering, tool call formatting, thinking trace display,
//! and slash command dispatch.

use opendev_models::message::{ChatMessage, Role};

use crate::app::{AppState, DisplayMessage, DisplayRole, DisplayToolCall};
// format_tool_call_display is available for future slash-command / display enhancements.
#[allow(unused_imports)]
use crate::formatters::tool_registry::format_tool_call_display;

/// Controller responsible for managing conversation message flow.
pub struct MessageController;

impl MessageController {
    pub fn new() -> Self {
        Self
    }

    /// Handle user message submission — adds it to the display state.
    pub fn handle_user_submit(&self, state: &mut AppState, text: &str) {
        state
            .messages
            .push(DisplayMessage::new(DisplayRole::User, text));
        // Reset scroll to follow new content
        state.scroll_offset = 0;
        state.user_scrolled = false;
    }

    /// Handle a streaming chunk from the agent.
    ///
    /// Appends to the last assistant message or creates a new one.
    pub fn handle_agent_chunk(&self, state: &mut AppState, text: &str) {
        if let Some(last) = state.messages.last_mut()
            && last.role == DisplayRole::Assistant
            && last.tool_call.is_none()
        {
            last.content.push_str(text);
            return;
        }
        // Start a new assistant message
        state
            .messages
            .push(DisplayMessage::new(DisplayRole::Assistant, text));
    }

    /// Handle a complete agent message (non-streaming path or final message).
    pub fn handle_agent_message(&self, state: &mut AppState, msg: ChatMessage) {
        let role = match msg.role {
            Role::User => DisplayRole::User,
            Role::Assistant => DisplayRole::Assistant,
            Role::System => DisplayRole::System,
        };

        let tool_call = msg.tool_calls.first().map(DisplayToolCall::from_model);

        state.messages.push(DisplayMessage {
            role,
            content: msg.content.clone(),
            tool_call,
            collapsed: false,
            thinking_started_at: None,
            thinking_duration_secs: None,
        });

        // Auto-scroll to latest message
        if !state.user_scrolled {
            state.scroll_offset = 0;
        }
    }
}

impl Default for MessageController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
