//! Injects plan mode status when agent is executing a plan.

use crate::attachments::{Attachment, CadenceGate, ContextCollector, TurnContext};
use crate::prompts::reminders::MessageClass;

pub struct PlanModeCollector {
    cadence: CadenceGate,
}

impl PlanModeCollector {
    pub fn new(interval: usize) -> Self {
        Self {
            cadence: CadenceGate::new(interval),
        }
    }
}

#[async_trait::async_trait]
impl ContextCollector for PlanModeCollector {
    fn name(&self) -> &'static str {
        "plan_mode"
    }

    fn should_fire(&self, ctx: &TurnContext<'_>) -> bool {
        if !self.cadence.should_fire(ctx.turn_number) {
            return false;
        }
        // Only fire if we're in an active planning phase
        if let Some(state) = ctx.shared_state
            && let Ok(state) = state.lock()
            && let Some(phase) = state.get("planning_phase").and_then(|v| v.as_str())
        {
            return matches!(phase, "explore" | "plan" | "executing");
        }
        false
    }

    async fn collect(&self, ctx: &TurnContext<'_>) -> Option<Attachment> {
        let state = ctx.shared_state?.lock().ok()?;
        let phase = state.get("planning_phase").and_then(|v| v.as_str())?;

        let content = match phase {
            "explore" => "You are in the exploration phase of planning. Focus on understanding \
                 the codebase before creating a plan. Use Explore subagents for \
                 multi-file analysis."
                .to_string(),
            "plan" => "You are in plan creation mode. Spawn a Planner subagent with your \
                 findings to create a detailed implementation plan."
                .to_string(),
            "executing" => "You are executing an approved plan. Continue working through your \
                 todo list items in order. Check your todo list for the next \
                 incomplete item."
                .to_string(),
            _ => return None,
        };

        Some(Attachment {
            name: "plan_mode",
            content,
            class: MessageClass::Nudge,
        })
    }

    fn did_fire(&self, turn: usize) {
        self.cadence.mark_fired(turn);
    }

    fn reset(&self) {
        self.cadence.reset();
    }
}
