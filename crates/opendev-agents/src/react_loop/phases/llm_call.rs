//! LLM call phase: build payload, execute HTTP, handle streaming.
//!
//! When streaming is supported, creates a `StreamingToolExecutor` that begins
//! executing read-only tools as soon as their arguments are complete — before
//! the full LLM response has finished. This overlaps tool execution with LLM
//! generation for lower per-iteration latency.

use std::sync::Arc;

use serde_json::Value;
use tracing::{Instrument, debug, info, info_span, warn};

use crate::llm_calls::LlmCaller;
use crate::traits::{AgentError, AgentResult, TaskMonitor};
use opendev_http::adapted_client::AdaptedClient;
use opendev_runtime::SessionDebugLogger;
use opendev_tools_core::{ToolContext, ToolRegistry};
use tokio_util::sync::CancellationToken;

use super::super::emitter::IterationEmitter;
use super::super::loop_state::LoopState;
use super::super::streaming_executor::StreamingToolExecutor;
use super::super::types::LoopAction;

/// Result of a successful LLM call.
pub(in crate::react_loop) struct LlmCallResult {
    /// Parsed response body.
    pub body: Value,
    /// Wall-clock latency in milliseconds.
    pub llm_latency_ms: u64,
    /// Streaming tool executor with early results (if streaming was used).
    /// `None` when the provider doesn't support streaming.
    pub streaming_executor: Option<StreamingToolExecutor>,
}

/// Execute the LLM call for this iteration.
///
/// Returns `Ok(LlmCallResult)` on success, or `Err(LoopAction)` when the
/// loop should continue (retryable) or return (interrupt/error).
#[allow(clippy::too_many_arguments, clippy::ptr_arg)]
pub(in crate::react_loop) async fn execute_llm_call<M>(
    caller: &LlmCaller,
    http_client: &AdaptedClient,
    messages: &mut Vec<Value>,
    tool_schemas: &[Value],
    state: &LoopState,
    emitter: &IterationEmitter<'_>,
    task_monitor: Option<&M>,
    cancel: Option<&CancellationToken>,
    debug_logger: Option<&SessionDebugLogger>,
    tool_registry: Option<&Arc<ToolRegistry>>,
    tool_context: Option<&ToolContext>,
) -> Result<LlmCallResult, LoopAction>
where
    M: TaskMonitor + ?Sized,
{
    // Build payload from the (possibly deferral-filtered) tool_schemas slice.
    // Tool schema deferral filters the set each iteration based on
    // activated_tools, so caching the full set would bypass deferral.
    let mut payload = caller.build_action_payload(messages, tool_schemas);
    if let Some(ref override_model) = state.skill_model_override {
        payload["model"] = serde_json::json!(override_model);
        debug!(iteration = state.iteration, model = %override_model, "Using skill model override");
    }
    debug!(iteration = state.iteration, model = %payload["model"], "ReAct iteration");

    let llm_start = std::time::Instant::now();
    let streaming = http_client.supports_streaming();
    debug!(streaming, "LLM call mode");

    // Debug log: outgoing LLM request
    if let Some(logger) = debug_logger {
        let model = payload["model"].as_str().unwrap_or("unknown");
        logger.log_llm_request(state.iteration, model, streaming, &payload);
    }

    // Create streaming tool executor if registry is available (for early read-only tool execution).
    let streaming_executor = if streaming {
        tool_registry.zip(tool_context).map(|(reg, ctx)| {
            StreamingToolExecutor::new(
                Arc::clone(reg),
                ctx.clone(),
                cancel.map(|c| c.child_token()),
            )
        })
    } else {
        None
    };

    let http_result = if streaming {
        let ui_cb = opendev_http::streaming::FnStreamCallback(|event| {
            use opendev_http::streaming::StreamEvent;
            match event {
                StreamEvent::TextDelta(text) => emitter.emit_text(text),
                StreamEvent::ReasoningDelta(text) => emitter.emit_reasoning(text),
                StreamEvent::ReasoningBlockStart => emitter.emit_reasoning_block_start(),
                _ => {}
            }
        });

        // If we have a streaming executor, compose it with the UI callback
        // so both receive stream events (tool completion events trigger early execution).
        if let Some(ref executor) = streaming_executor {
            let composite =
                opendev_http::streaming::CompositeStreamCallback::new(vec![&ui_cb, executor]);
            async {
                http_client
                    .post_json_streaming(&payload, cancel, &composite)
                    .await
                    .map_err(|e| AgentError::LlmError(e.to_string()))
            }
            .instrument(info_span!(
                "llm_call",
                iteration = state.iteration,
                model = %payload["model"],
            ))
            .await
            .map_err(|e| LoopAction::Return(Err(e)))?
        } else {
            async {
                http_client
                    .post_json_streaming(&payload, cancel, &ui_cb)
                    .await
                    .map_err(|e| AgentError::LlmError(e.to_string()))
            }
            .instrument(info_span!(
                "llm_call",
                iteration = state.iteration,
                model = %payload["model"],
            ))
            .await
            .map_err(|e| LoopAction::Return(Err(e)))?
        }
    } else {
        async {
            http_client
                .post_json(&payload, cancel)
                .await
                .map_err(|e| AgentError::LlmError(e.to_string()))
        }
        .instrument(info_span!(
            "llm_call",
            iteration = state.iteration,
            model = %payload["model"],
        ))
        .await
        .map_err(|e| LoopAction::Return(Err(e)))?
    };
    let llm_latency_ms = llm_start.elapsed().as_millis() as u64;

    // Handle interruption
    if http_result.interrupted {
        if task_monitor.is_some_and(|m| m.is_background_requested()) {
            info!(
                iteration = state.iteration,
                "Background requested during LLM call — yielding"
            );
            return Err(LoopAction::Return(Ok(AgentResult::backgrounded(
                messages.clone(),
            ))));
        }
        return Err(LoopAction::Return(Ok(AgentResult::interrupted(
            messages.clone(),
        ))));
    }

    // Handle HTTP failure
    if !http_result.success {
        let err_msg = http_result
            .error
            .as_deref()
            .unwrap_or("HTTP request failed");
        warn!(error = err_msg, "LLM HTTP call failed");
        if let Some(logger) = debug_logger {
            logger.log_llm_error(state.iteration, err_msg);
        }
        if http_result.retryable {
            return Err(LoopAction::Continue);
        }
        return Err(LoopAction::Return(Err(AgentError::LlmError(
            err_msg.to_string(),
        ))));
    }

    // Extract body
    let body = http_result.body.ok_or_else(|| {
        LoopAction::Return(Err(AgentError::LlmError("Empty response body".to_string())))
    })?;

    // Check for API error in response body
    if let Some(error_obj) = body.get("error") {
        let msg = error_obj
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown API error");
        if let Some(logger) = debug_logger {
            logger.log_llm_error(state.iteration, msg);
        }
        return Err(LoopAction::Return(Err(AgentError::LlmError(format!(
            "API error: {msg}"
        )))));
    }

    // Debug log: incoming LLM response
    if let Some(logger) = debug_logger {
        let input_tokens = body
            .get("usage")
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        let output_tokens = body
            .get("usage")
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        logger.log_llm_response(
            state.iteration,
            llm_latency_ms,
            input_tokens,
            output_tokens,
            &body,
        );
    }

    // Wait for any early-started tools to complete before returning.
    if let Some(ref executor) = streaming_executor
        && executor.has_running_tasks()
    {
        debug!("Waiting for early-started streaming tools to complete");
        executor.wait_for_completion().await;
    }

    Ok(LlmCallResult {
        body,
        llm_latency_ms,
        streaming_executor,
    })
}
