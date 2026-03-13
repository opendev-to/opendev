//! Present plan tool — present a plan file for user approval.
//!
//! After a Planner subagent writes a plan to a file, the main agent calls
//! this tool to present the plan and get user sign-off before implementation.
//!
//! On approval the tool:
//! 1. Parses implementation steps from the plan markdown
//! 2. Creates todo items from those steps (via a shared `TodoManager`)
//! 3. Registers the plan in the `PlanIndex` for session tracking
//! 4. Stores the plan file to `~/.opendev/plans/`

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use opendev_runtime::{PlanIndex, TodoManager, parse_plan_steps};
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Minimum plan length in characters to be considered valid.
const MIN_PLAN_LENGTH: usize = 100;

/// Tool for presenting a completed plan to the user for approval.
#[derive(Debug)]
pub struct PresentPlanTool {
    /// Shared todo manager — steps are created here on approval.
    todo_manager: Option<Arc<Mutex<TodoManager>>>,
}

impl PresentPlanTool {
    /// Create a present_plan tool without todo integration (auto-approve only).
    pub fn new() -> Self {
        Self { todo_manager: None }
    }

    /// Create a present_plan tool with a shared todo manager.
    ///
    /// When a plan is approved, its implementation steps will be added
    /// as todos to this manager.
    pub fn with_todo_manager(manager: Arc<Mutex<TodoManager>>) -> Self {
        Self {
            todo_manager: Some(manager),
        }
    }

    /// Plans directory: `~/.opendev/plans/`.
    fn plans_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".opendev").join("plans"))
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
        "present_plan"
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
            };
        }

        // Parse implementation steps into todos
        let steps = parse_plan_steps(&plan_content);
        let step_count = steps.len();

        // Create todos from plan steps
        if let Some(ref mgr) = self.todo_manager
            && let Ok(mut todo_mgr) = mgr.lock()
        {
            todo_mgr.clear(); // Clear any previous plan's todos
            for step in &steps {
                todo_mgr.add(step.clone());
            }
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
        metadata.insert("plan_file_path".into(), serde_json::json!(plan_file_path));
        metadata.insert("plan_length".into(), serde_json::json!(plan_content.len()));
        metadata.insert("step_count".into(), serde_json::json!(step_count));

        if let Some(ref name) = plan_name {
            metadata.insert("plan_name".into(), serde_json::json!(name));
        }

        // Format step list for agent context
        let step_list = if steps.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = steps
                .iter()
                .enumerate()
                .map(|(i, s)| format!("  {}. {s}", i + 1))
                .collect();
            format!(
                "\n\nTodo items created ({step_count}):\n{}",
                items.join("\n")
            )
        };

        let plan_name_display = plan_name
            .as_deref()
            .map(|n| format!(" ({n})"))
            .unwrap_or_default();

        ToolResult::ok_with_metadata(
            format!(
                "Plan completed{plan_name_display} ({} chars, {step_count} steps). \
                 Proceed with implementation.\n\n\
                 Plan file: {plan_file_path}{step_list}",
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
mod tests {
    use super::*;

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_present_plan_missing_path() {
        let tool = PresentPlanTool::new();
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("plan_file_path is required"));
    }

    #[tokio::test]
    async fn test_present_plan_file_not_found() {
        let tool = PresentPlanTool::new();
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "plan_file_path",
            serde_json::json!("/tmp/nonexistent_plan.md"),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_present_plan_empty_file() {
        let tool = PresentPlanTool::new();
        let ctx = ToolContext::new("/tmp");

        let path = std::env::temp_dir().join("test_empty_plan_rs.md");
        std::fs::write(&path, "").unwrap();

        let args = make_args(&[("plan_file_path", serde_json::json!(path.to_string_lossy()))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("empty"));

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn test_present_plan_too_short() {
        let tool = PresentPlanTool::new();
        let ctx = ToolContext::new("/tmp");

        let path = std::env::temp_dir().join("test_short_plan_rs.md");
        std::fs::write(&path, "A short plan.").unwrap();

        let args = make_args(&[("plan_file_path", serde_json::json!(path.to_string_lossy()))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("too short"));

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn test_present_plan_missing_delimiter() {
        let tool = PresentPlanTool::new();
        let ctx = ToolContext::new("/tmp");

        let path = std::env::temp_dir().join("test_no_delimiter_plan_rs.md");
        let content = "x".repeat(200);
        std::fs::write(&path, &content).unwrap();

        let args = make_args(&[("plan_file_path", serde_json::json!(path.to_string_lossy()))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("---BEGIN PLAN---"));

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn test_present_plan_valid_creates_todos() {
        let todo_mgr = Arc::new(Mutex::new(TodoManager::new()));
        let tool = PresentPlanTool::with_todo_manager(Arc::clone(&todo_mgr));
        let ctx = ToolContext::new("/tmp");

        let path = std::env::temp_dir().join("test_valid_plan_todos_rs.md");
        let content = format!(
            "# Plan\n\n---BEGIN PLAN---\n\n## Implementation Steps\n\n\
             1. First step\n2. Second step\n3. Third step\n\n\
             ## Verification\n\n1. Run tests\n2. Check lint\n\n\
             ---END PLAN---\n\n{}\n",
            "Additional details. ".repeat(10)
        );
        std::fs::write(&path, &content).unwrap();

        let args = make_args(&[("plan_file_path", serde_json::json!(path.to_string_lossy()))]);
        let result = tool.execute(args, &ctx).await;
        assert!(result.success, "Error: {:?}", result.error);
        assert!(
            result
                .output
                .as_ref()
                .unwrap()
                .contains("Proceed with implementation")
        );
        assert_eq!(
            result.metadata.get("plan_approved"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            result.metadata.get("step_count"),
            Some(&serde_json::json!(3))
        );

        // Verify todos were created
        let mgr = todo_mgr.lock().unwrap();
        assert_eq!(mgr.total(), 3);
        assert_eq!(mgr.all()[0].title, "First step");
        assert_eq!(mgr.all()[2].title, "Third step");

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn test_present_plan_valid_no_todo_manager() {
        let tool = PresentPlanTool::new();
        let ctx = ToolContext::new("/tmp");

        let path = std::env::temp_dir().join("test_valid_plan_no_todo_rs.md");
        let content = format!(
            "# Plan\n\n---BEGIN PLAN---\n\n## Implementation Steps\n\n\
             1. First step\n2. Second step\n3. Third step\n\n\
             ## Verification\n\n1. Run tests\n2. Check lint\n\n\
             ---END PLAN---\n\n{}\n",
            "Additional details. ".repeat(10)
        );
        std::fs::write(&path, &content).unwrap();

        let args = make_args(&[("plan_file_path", serde_json::json!(path.to_string_lossy()))]);
        let result = tool.execute(args, &ctx).await;
        assert!(result.success, "Error: {:?}", result.error);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/test/plan.md");
        assert!(!expanded.to_string_lossy().starts_with('~'));

        let no_tilde = expand_tilde("/absolute/path");
        assert_eq!(no_tilde, PathBuf::from("/absolute/path"));
    }
}
