//! Response processing: parse response, emit content, track cost/tokens.

use std::sync::Mutex;

use serde_json::Value;

use crate::llm_calls::LlmCaller;
use crate::traits::{AgentError, LlmResponse, TaskMonitor};
use opendev_context::ContextCompactor;
use opendev_runtime::{CostTracker, PricingInfo, TokenUsage};

use super::super::ReactLoop;
use super::super::emitter::IterationEmitter;
use super::super::loop_state::LoopState;
use super::super::types::{IterationMetrics, TurnResult};

/// Result of processing the LLM response.
pub(in crate::react_loop) struct ProcessedResponse {
    /// The parsed LLM response (needed for finish_reason in completion handling).
    pub response: LlmResponse,
    /// The turn decision from `process_iteration`.
    pub turn: TurnResult,
    /// Per-iteration metrics (tool_calls will be filled in later).
    pub iter_metrics: IterationMetrics,
}

/// Parse the LLM response, emit content/tokens, track cost, and process the iteration.
#[allow(clippy::too_many_arguments)]
pub(in crate::react_loop) fn process_response<M>(
    react_loop: &ReactLoop,
    caller: &LlmCaller,
    body: &Value,
    llm_latency_ms: u64,
    messages: &mut Vec<Value>,
    state: &mut LoopState,
    emitter: &IterationEmitter<'_>,
    task_monitor: Option<&M>,
    cost_tracker: Option<&Mutex<CostTracker>>,
    compactor: Option<&Mutex<ContextCompactor>>,
) -> Result<ProcessedResponse, AgentError>
where
    M: TaskMonitor + ?Sized,
{
    let response = caller.parse_action_response(body);

    // Extract token counts
    let input_tokens = response
        .usage
        .as_ref()
        .and_then(|u| u.get("prompt_tokens").and_then(|t| t.as_u64()))
        .unwrap_or(0);
    let output_tokens = response
        .usage
        .as_ref()
        .and_then(|u| u.get("completion_tokens").and_then(|t| t.as_u64()))
        .unwrap_or(0);

    // Emit reasoning/text content (non-streaming fallback)
    if let Some(ref reasoning) = response.reasoning_content {
        emitter.emit_reasoning_if_not_streamed(reasoning);
    }
    if let Some(ref content) = response.content {
        emitter.emit_text_if_not_streamed(content);
    }

    // Track token usage via task monitor
    if let Some(monitor) = task_monitor
        && let Some(ref usage) = response.usage
        && let Some(total) = usage.get("total_tokens").and_then(|t| t.as_u64())
    {
        monitor.update_tokens(total);
    }

    // Emit token usage event
    if input_tokens > 0 || output_tokens > 0 {
        emitter.emit_token_usage(input_tokens, output_tokens);
    }

    // Record cost tracking
    if let Some(ct) = cost_tracker
        && let Some(ref usage_json) = response.usage
    {
        let token_usage = TokenUsage::from_json(usage_json);
        let pricing = body
            .get("model")
            .and_then(|m| m.as_str())
            .and_then(PricingInfo::for_model);
        if let Ok(mut tracker) = ct.lock() {
            tracker.record_usage(&token_usage, pricing.as_ref());
        }
    }

    // Calibrate compactor with real API token counts (input + output combined)
    let total_tokens = input_tokens + output_tokens;
    if let Some(comp) = compactor
        && total_tokens > 0
        && let Ok(mut c) = comp.lock()
    {
        c.update_from_api_usage(total_tokens, messages.len());
        emitter.emit_context_usage(c.usage_pct());
    }

    // Initialize per-iteration metrics
    let iter_metrics = IterationMetrics {
        iteration: state.iteration,
        llm_latency_ms,
        input_tokens,
        output_tokens,
        tool_calls: Vec::new(),
        total_duration_ms: 0,
    };

    // Process the iteration (determine turn result)
    let turn = react_loop.process_iteration(
        &response,
        messages,
        state.iteration,
        &mut state.consecutive_no_tool_calls,
    )?;

    Ok(ProcessedResponse {
        response,
        turn,
        iter_metrics,
    })
}
