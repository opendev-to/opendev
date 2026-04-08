//! SubagentRunner trait and implementations.
//!
//! Defines a trait for react loop strategies so each subagent type
//! can have its own loop. `StandardReactRunner` wraps the existing
//! `ReactLoop` for General/Planner/Build agents, while `SimpleReactRunner`
//! provides a stripped-down loop for Explore.

mod simple;
mod standard;

pub use simple::SimpleReactRunner;
pub use standard::StandardReactRunner;

use async_trait::async_trait;
use serde_json::Value;

use std::sync::Arc;

use crate::llm_calls::LlmCaller;
use crate::traits::{AgentError, AgentEventCallback, AgentResult};
use opendev_http::adapted_client::AdaptedClient;
use opendev_runtime::{Mailbox, SessionDebugLogger, ToolApprovalSender};
use opendev_tools_core::{ToolContext, ToolRegistry};
use tokio_util::sync::CancellationToken;

/// Dependencies bundled for the runner (avoids many-param functions).
pub struct RunnerContext<'a> {
    pub caller: &'a LlmCaller,
    pub http_client: &'a AdaptedClient,
    pub tool_schemas: &'a [Value],
    pub tool_registry: &'a Arc<ToolRegistry>,
    pub tool_context: &'a ToolContext,
    pub event_callback: Option<&'a dyn AgentEventCallback>,
    pub cancel: Option<&'a CancellationToken>,
    pub tool_approval_tx: Option<&'a ToolApprovalSender>,
    pub debug_logger: Option<&'a SessionDebugLogger>,
    /// Optional mailbox for team members to receive messages.
    pub mailbox: Option<&'a Mailbox>,
}

/// Inject mailbox messages into the LLM message history.
///
/// Converts received `MailboxMessage` entries into user-role messages
/// that the LLM will process on its next turn.
fn inject_mailbox_messages(msgs: Vec<opendev_runtime::MailboxMessage>, messages: &mut Vec<Value>) {
    for msg in msgs {
        let content = match msg.msg_type {
            opendev_runtime::MessageType::ShutdownRequest => format!(
                "[TEAM SHUTDOWN REQUEST from '{}']: {}\n\
                 Wrap up your current work and call task_complete.",
                msg.from, msg.content
            ),
            opendev_runtime::MessageType::Text => {
                format!("[Message from teammate '{}']: {}", msg.from, msg.content)
            }
            _ => continue,
        };
        messages.push(serde_json::json!({ "role": "user", "content": content }));
    }
}

/// Trait for react loop strategies.
#[async_trait]
pub trait SubagentRunner: Send + Sync {
    /// Run the react loop over the given message history.
    async fn run(
        &self,
        ctx: &RunnerContext<'_>,
        messages: &mut Vec<Value>,
    ) -> Result<AgentResult, AgentError>;

    /// Name of this runner (for logging).
    fn name(&self) -> &str;
}
