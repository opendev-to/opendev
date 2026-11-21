//! Safety checks: iteration limits, interrupt, background yield, auto-compaction.

use std::sync::Mutex;

use serde_json::Value;
use tracing::{info, warn};

use crate::llm_calls::LlmCaller;
use crate::prompts::reminders::{append_directive, get_reminder};
use crate::traits::{AgentError, AgentResult, TaskMonitor};
use opendev_context::ContextCompactor;
use opendev_http::adapted_client::AdaptedClient;
use opendev_runtime::{CostTracker, PricingInfo, TokenUsage};
use tokio_util::sync::CancellationToken;

use super::super::ReactLoop;
use super::super::compaction::{apply_staged_compaction, do_llm_compaction};
use super::super::loop_state::LoopState;

/// Check safety conditions at the start of each iteration.
///
/// Returns `Some(result)` if the loop should exit (max iterations reached,
/// interrupt requested, or background yield). Returns `None` to continue.
#[allow(clippy::too_many_arguments)]
pub(in crate::react_loop) async fn check_safety<M>(
    react_loop: &ReactLoop,
    caller: &LlmCaller,
    http_client: &AdaptedClient,
    messages: &mut Vec<Value>,
    state: &mut LoopState,
    task_monitor: Option<&M>,
    cost_tracker: Option<&Mutex<CostTracker>>,
    compactor: Option<&Mutex<ContextCompactor>>,
    cancel: Option<&CancellationToken>,
) -> Option<Result<AgentResult, AgentError>>
where
    M: TaskMonitor + ?Sized,
{
    // --- Max iterations wind-down ---
    if react_loop.check_iteration_limit(state.iteration) {
        info!(
            iteration = state.iteration,
            "Max iterations reached — requesting wind-down summary"
        );

        let summary_prompt = get_reminder("safety_limit_summary", &[]);
        append_directive(messages, &summary_prompt);

        // Build payload WITHOUT tools to force text-only response
        let mut payload = caller.build_action_payload(messages, &[]);
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("tool_choice");
            obj.remove("tools");
            obj.remove("_reasoning_effort");
        }

        match http_client.post_json(&payload, cancel).await {
            Ok(http_result) if http_result.success => {
                if let Some(body) = http_result.body {
                    let response = caller.parse_action_response(&body);

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

                    if let Some(content) = &response.content {
                        let wind_down_msg = format!(
                            "[Max iterations ({}) reached — summary below]\n\n{}",
                            state.iteration - 1,
                            content
                        );
                        return Some(Ok(AgentResult::ok(wind_down_msg, messages.clone())));
                    }
                }
            }
            Ok(_) | Err(_) => {
                warn!("Wind-down LLM call failed, falling back to last content");
            }
        }

        let last_content = messages
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("Max iterations reached.")
            .to_string();
        let wind_down_msg = format!(
            "[Max iterations ({}) reached — summary unavailable]\n\n{}",
            state.iteration - 1,
            last_content
        );
        return Some(Ok(AgentResult::ok(wind_down_msg, messages.clone())));
    }

    // --- Interrupt check ---
    if let Some(monitor) = task_monitor
        && monitor.should_interrupt()
    {
        return Some(Ok(AgentResult::interrupted(messages.clone())));
    }

    // --- Background yield ---
    if let Some(monitor) = task_monitor
        && monitor.is_background_requested()
    {
        info!(
            iteration = state.iteration,
            "Background requested — yielding to foreground"
        );
        return Some(Ok(AgentResult::backgrounded(messages.clone())));
    }

    // --- Auto-compaction ---
    if let Some(comp) = compactor {
        let needs_llm = apply_staged_compaction(comp, messages);
        if needs_llm {
            do_llm_compaction(comp, messages, caller, http_client).await;
            state
                .subdir_tracker
                .reset_after_compaction(&state.startup_paths, messages);
            // Signal compaction to collectors and reset their cadence
            state
                .compaction_flag
                .store(true, std::sync::atomic::Ordering::Relaxed);
            state.collector_runner.reset_all();
            info!(
                injected_remaining = state.subdir_tracker.injected_count(),
                "Reset instruction tracker after LLM compaction"
            );
        }
    }

    None
}
