//! Utility methods on ReactLoop: response processing, tool classification, metrics.

mod analysis;
mod tools;

use serde_json::Value;
use tracing::{debug, warn};

use crate::traits::LlmResponse;

use super::ReactLoop;
use super::types::{IterationMetrics, TurnResult};

impl ReactLoop {
    /// Set the user's original task text for completion nudge context.
    ///
    /// Call this before each `run()` so the implicit completion nudge
    /// can verify the original request was fully addressed.
    pub fn set_original_task(&mut self, original_task: Option<String>) {
        self.config.original_task = original_task;
    }

    /// Return a snapshot of accumulated iteration metrics collected during `run()`.
    pub fn iteration_metrics(&self) -> Vec<IterationMetrics> {
        self.iteration_metrics.lock().unwrap().clone()
    }

    /// Clear accumulated iteration metrics.
    pub fn clear_metrics(&self) {
        self.iteration_metrics.lock().unwrap().clear();
    }

    /// Push a new iteration metrics entry.
    pub(super) fn push_metrics(&self, metrics: IterationMetrics) {
        self.iteration_metrics.lock().unwrap().push(metrics);
    }

    /// Process a single LLM response and determine the next action.
    ///
    /// This is the core decision function of the ReAct loop. It examines
    /// the LLM response and returns a `TurnResult` indicating what should
    /// happen next.
    pub fn process_response(
        &self,
        response: &LlmResponse,
        consecutive_no_tool_calls: usize,
    ) -> TurnResult {
        if response.interrupted {
            return TurnResult::Interrupted;
        }

        if !response.success {
            // Failed API call — if we have an error, treat as needing continuation
            warn!(
                error = response.error.as_deref().unwrap_or("unknown"),
                "LLM call failed"
            );
            return TurnResult::Continue;
        }

        // Check for tool calls
        let tool_calls = response.tool_calls.as_ref().and_then(|tcs| {
            if tcs.is_empty() {
                None
            } else {
                Some(tcs.clone())
            }
        });

        match tool_calls {
            Some(tcs) => TurnResult::ToolCall { tool_calls: tcs },
            None => {
                // No tool calls — check if we should accept completion
                let content = response.content.as_deref().unwrap_or("Done.").to_string();

                if consecutive_no_tool_calls >= self.config.max_nudge_attempts {
                    debug!("Max nudge attempts reached, accepting completion");
                    TurnResult::Complete {
                        content,
                        status: None,
                    }
                } else {
                    // Still have nudge budget — caller decides whether to nudge
                    TurnResult::Complete {
                        content,
                        status: None,
                    }
                }
            }
        }
    }

    /// Check if the iteration limit has been reached.
    pub fn check_iteration_limit(&self, iteration: usize) -> bool {
        match self.config.max_iterations {
            Some(max) => iteration > max,
            None => false,
        }
    }

    /// Process a single iteration given an already-parsed LLM response.
    ///
    /// This is the preferred integration point. The caller makes the HTTP
    /// request, parses the response, then calls this method to determine
    /// the next action.
    pub fn process_iteration(
        &self,
        response: &LlmResponse,
        messages: &mut Vec<Value>,
        iteration: usize,
        consecutive_no_tool_calls: &mut usize,
    ) -> Result<TurnResult, crate::traits::AgentError> {
        if self.check_iteration_limit(iteration) {
            return Ok(TurnResult::MaxIterations);
        }

        if response.interrupted {
            return Ok(TurnResult::Interrupted);
        }

        if !response.success {
            return Err(crate::traits::AgentError::LlmError(
                response
                    .error
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        // Append assistant message to history
        if let Some(ref msg) = response.message {
            let raw_content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
            let mut assistant_msg = serde_json::json!({
                "role": "assistant",
                "content": raw_content,
            });
            if let Some(tool_calls) = msg.get("tool_calls")
                && !tool_calls.is_null()
            {
                assistant_msg["tool_calls"] = tool_calls.clone();
            }
            if let Some(reasoning) = msg.get("reasoning_content")
                && !reasoning.is_null()
            {
                assistant_msg["reasoning_content"] = reasoning.clone();
            }
            messages.push(assistant_msg);
        }

        let turn = self.process_response(response, *consecutive_no_tool_calls);

        match &turn {
            TurnResult::ToolCall { .. } => {
                *consecutive_no_tool_calls = 0;
            }
            TurnResult::Complete { .. } => {
                *consecutive_no_tool_calls += 1;
            }
            _ => {}
        }

        Ok(turn)
    }
}

#[cfg(test)]
mod tests;
