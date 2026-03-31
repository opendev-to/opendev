//! StandardReactRunner — wraps existing ReactLoop for General/Planner/Build agents.

use async_trait::async_trait;
use serde_json::Value;

use super::{RunnerContext, SubagentRunner};
use crate::react_loop::{ReactLoop, ReactLoopConfig};
use crate::traits::{AgentError, AgentResult, TaskMonitor};

/// Wraps the existing `ReactLoop` for General, Planner, Build agents.
///
/// Delegates to `ReactLoop::run()` with subagent-appropriate config
/// (no cost_tracker, no compactor, no todo_manager, no approval gates).
pub struct StandardReactRunner {
    config: ReactLoopConfig,
}

impl StandardReactRunner {
    /// Create a new standard runner with the given config.
    pub fn new(config: ReactLoopConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SubagentRunner for StandardReactRunner {
    async fn run(
        &self,
        ctx: &RunnerContext<'_>,
        messages: &mut Vec<Value>,
    ) -> Result<AgentResult, AgentError> {
        // Drain mailbox messages before starting the react loop (for team members)
        if let Some(mailbox) = ctx.mailbox
            && let Ok(msgs) = mailbox.receive()
        {
            super::inject_mailbox_messages(msgs, messages);
        }

        let react_loop = ReactLoop::new(self.config.clone());

        react_loop
            .run(
                ctx.caller,
                ctx.http_client,
                messages,
                ctx.tool_schemas,
                ctx.tool_registry,
                ctx.tool_context,
                None::<&dyn TaskMonitor>,
                ctx.event_callback,
                None, // no cost_tracker
                None, // no artifact_index
                None, // no compactor
                None, // no todo_manager
                ctx.cancel,
                ctx.tool_approval_tx,
                ctx.debug_logger,
            )
            .await
    }

    fn name(&self) -> &str {
        "StandardReactRunner"
    }
}

#[cfg(test)]
#[path = "standard_tests.rs"]
mod tests;
