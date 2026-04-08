//! Present plan tool — present a plan file for user approval.
//!
//! After a Planner subagent writes a plan to a file, the main agent calls
//! this tool to present the plan and get user sign-off before implementation.
//!
//! On approval the tool:
//! 1. Registers the plan in the `PlanIndex` for session tracking
//! 2. Stores the plan file to `~/.opendev/plans/`
//!
//! Todo creation is deferred to the LLM's `write_todos` call after approval,
//! which produces better hierarchical grouping than automated parsing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use opendev_runtime::{PlanApprovalRequest, PlanApprovalSender, PlanIndex, TodoManager};
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Minimum plan length in characters to be considered valid.
const MIN_PLAN_LENGTH: usize = 100;

/// Tool for presenting a completed plan to the user for approval.
#[derive(Debug)]
pub struct PresentPlanTool {
    /// Shared todo manager — steps are created here on approval.
    todo_manager: Option<Arc<Mutex<TodoManager>>>,
    /// Channel to send plan approval requests to the TUI.
    /// When `Some`, the tool blocks until the user approves/rejects.
    /// When `None` (headless/non-interactive), auto-approve.
    approval_tx: Option<PlanApprovalSender>,
}

impl PresentPlanTool {
    /// Create a present_plan tool without todo integration (auto-approve only).
    pub fn new() -> Self {
        Self {
            todo_manager: None,
            approval_tx: None,
        }
    }

    /// Create a present_plan tool with a shared todo manager.
    ///
    /// When a plan is approved, its implementation steps will be added
    /// as todos to this manager.
    pub fn with_todo_manager(manager: Arc<Mutex<TodoManager>>) -> Self {
        Self {
            todo_manager: Some(manager),
            approval_tx: None,
        }
    }

    /// Attach a plan approval channel for interactive (TUI) mode.
    ///
    /// When set, `execute()` sends the plan content through this channel
    /// and blocks until the user approves, rejects, or requests modification.
    pub fn with_approval_tx(mut self, tx: PlanApprovalSender) -> Self {
        self.approval_tx = Some(tx);
        self
    }

    /// Plans directory: `~/.opendev/plans/`.
    fn plans_dir() -> Option<PathBuf> {
        Some(opendev_config::Paths::default().global_plans_dir())
    }
}

impl Default for PresentPlanTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl BaseTool for PresentPlanTool {
    fn name(&self) -> &str {
        "EnterPlanMode"
    }

    fn description(&self) -> &str {
        "Present a completed plan file to the user for approval. \
         The plan must have ---BEGIN PLAN--- / ---END PLAN--- delimiters, \
         implementation steps, and a verification section."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "plan_file_path": {
                    "type": "string",
                    "description": "Absolute path to the plan file"
                }
            },
            "required": ["plan_file_path"]
        })
    }

    fn category(&self) -> opendev_tools_core::ToolCategory {
        opendev_tools_core::ToolCategory::Meta
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let plan_file_path = match args.get("plan_file_path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p,
            _ => return ToolResult::fail("plan_file_path is required"),
        };

        // Expand ~ and resolve path
        let plan_path = expand_tilde(plan_file_path);

        if !plan_path.exists() {
            return ToolResult {
                success: false,
                output: Some(
                    "Plan file does not exist. Spawn a Planner subagent \
                     to create the plan first."
                        .to_string(),
                ),
                error: Some(format!("Plan file not found: {plan_file_path}")),
                metadata: HashMap::new(),
                duration_ms: None,
                llm_suffix: None,
            };
        }

        // Read plan content
        let plan_content = match std::fs::read_to_string(&plan_path) {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to read plan file: {e}")),
        };

        // Validate plan is not empty
        let stripped = plan_content.trim();
        if stripped.is_empty() {
            return ToolResult {
                success: false,
                output: Some(
                    "Plan file exists but is empty. Spawn a Planner subagent \
                     to write the plan first."
                        .to_string(),
                ),
                error: Some(format!("Plan file is empty: {plan_file_path}")),
                metadata: HashMap::new(),
                duration_ms: None,
                llm_suffix: None,
            };
        }

        // Validate minimum length
        if stripped.len() < MIN_PLAN_LENGTH {
            return ToolResult {
                success: false,
                output: Some(format!(
                    "Plan file exists but contains insufficient content. \
                     Re-spawn the Planner subagent to write a detailed plan \
                     to {plan_file_path}."
                )),
                error: Some(format!(
                    "Plan file content is too short ({} chars). \
                     The Planner subagent likely didn't write a complete plan.",
                    stripped.len()
                )),
                metadata: HashMap::new(),
                duration_ms: None,
                llm_suffix: None,
            };
        }

        // Validate plan has required delimiters
        if !plan_content.contains("---BEGIN PLAN---") {
            return ToolResult {
                success: false,
                output: Some(format!(
                    "Plan file does not follow the required format. \
                     Re-spawn the Planner subagent and ensure it writes \
                     the plan with ---BEGIN PLAN--- / ---END PLAN--- delimiters \
                     to {plan_file_path}."
                )),
                error: Some("Plan is missing the required ---BEGIN PLAN--- delimiter.".to_string()),
                metadata: HashMap::new(),
                duration_ms: None,
                llm_suffix: None,
            };
        }

        // Validate plan has implementation steps
        let has_steps = plan_content.contains("## Implementation Steps")
            || plan_content.contains("## Steps")
            || plan_content.contains("## implementation steps");

        if !has_steps {
            return ToolResult {
                success: false,
                output: Some(format!(
                    "Plan file has the delimiters but no '## Implementation Steps' \
                     with numbered items. Re-spawn the Planner subagent to write \
                     a properly structured plan to {plan_file_path}."
                )),
                error: Some("Plan has no parseable implementation steps.".to_string()),
                metadata: HashMap::new(),
                duration_ms: None,
                llm_suffix: None,
            };
        }

        // Validate plan has verification section
        let has_verification = plan_content.contains("## Verification")
            || plan_content.contains("## verification")
            || plan_content.contains("## Testing");

        if !has_verification {
            return ToolResult {
                success: false,
                output: Some(format!(
                    "Plan needs a '## Verification' section with concrete test commands, \
                     build/lint checks, and manual verification steps. \
                     Re-spawn the Planner subagent to improve the verification section \
                     in {plan_file_path}."
                )),
                error: Some("Plan verification section is missing or too brief.".to_string()),
                metadata: HashMap::new(),
                duration_ms: None,
                llm_suffix: None,
            };
        }

        // --- Interactive approval: block until user decides ---
        let auto_approve_mode;
        if let Some(ref tx) = self.approval_tx {
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
            if tx
                .send(PlanApprovalRequest {
                    plan_content: plan_content.clone(),
                    response_tx: resp_tx,
                })
                .is_err()
            {
                // TUI closed — fall through to auto-approve
                auto_approve_mode = true;
            } else {
                match resp_rx.await {
                    Ok(decision) => match decision.action.as_str() {
                        "approve_auto" => {
                            auto_approve_mode = true;
                        }
                        "approve" => {
                            auto_approve_mode = false;
                        }
                        _ => {
                            // "modify" — user wants revisions
                            let mut metadata = HashMap::new();
                            metadata.insert("plan_approved".into(), serde_json::json!(false));
                            metadata
                                .insert("requires_modification".into(), serde_json::json!(true));
                            metadata
                                .insert("plan_file_path".into(), serde_json::json!(plan_file_path));
                            if !decision.feedback.is_empty() {
                                metadata.insert(
                                    "feedback".into(),
                                    serde_json::json!(decision.feedback),
                                );
                            }
                            return ToolResult {
                                success: false,
                                output: Some(
                                    "User requested plan revision. Re-spawn the Planner \
                                     subagent to revise the plan."
                                        .into(),
                                ),
                                error: None,
                                metadata,
                                duration_ms: None,
                                llm_suffix: None,
                            };
                        }
                    },
                    Err(_) => {
                        // Channel dropped — fall through to auto-approve
                        auto_approve_mode = true;
                    }
                }
            }
        } else {
            // Headless / non-interactive — auto-approve
            auto_approve_mode = true;
        }

        // Clear any previous plan's todos — the LLM will create new ones
        // via write_todos after approval, with proper hierarchical grouping.
        if let Some(ref mgr) = self.todo_manager
            && let Ok(mut todo_mgr) = mgr.lock()
        {
            todo_mgr.clear();
        }

        // Register in PlanIndex and copy plan to ~/.opendev/plans/
        let plan_name = if let Some(plans_dir) = Self::plans_dir() {
            let name = opendev_runtime::generate_plan_name(Some(&plans_dir), 50);

            // Copy plan file to plans directory
            if let Err(e) = std::fs::create_dir_all(&plans_dir) {
                tracing::warn!("Failed to create plans dir: {e}");
            } else {
                let dest = plans_dir.join(format!("{name}.md"));
                if let Err(e) = std::fs::copy(&plan_path, &dest) {
                    tracing::warn!("Failed to copy plan to {}: {e}", dest.display());
                }
            }

            // Register in index
            let index = PlanIndex::new(&plans_dir);
            let session_id = ctx.session_id.as_deref();
            let project_path = Some(ctx.working_dir.to_string_lossy().to_string());
            index.add_entry(&name, session_id, project_path.as_deref());

            Some(name)
        } else {
            None
        };

        // Build metadata
        let mut metadata = HashMap::new();
        metadata.insert("plan_approved".into(), serde_json::json!(true));
        metadata.insert("auto_approve".into(), serde_json::json!(auto_approve_mode));
        metadata.insert("plan_file_path".into(), serde_json::json!(plan_file_path));
        metadata.insert("plan_length".into(), serde_json::json!(plan_content.len()));
        metadata.insert("plan_content".into(), serde_json::json!(plan_content));

        if let Some(ref name) = plan_name {
            metadata.insert("plan_name".into(), serde_json::json!(name));
        }

        let plan_name_display = plan_name
            .as_deref()
            .map(|n| format!(" ({n})"))
            .unwrap_or_default();

        ToolResult::ok_with_metadata(
            format!(
                "Plan approved{plan_name_display} ({} chars). \
                 Proceed with implementation.\n\n\
                 Plan file: {plan_file_path}",
                plan_content.len()
            ),
            metadata,
        )
    }
}

/// Expand `~` to the home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(&path[2..]);
    }
    PathBuf::from(path)
}

#[cfg(test)]
#[path = "present_plan_tests.rs"]
mod tests;
