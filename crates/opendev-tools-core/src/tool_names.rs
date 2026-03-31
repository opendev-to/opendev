//! Canonical tool name constants.
//!
//! All tool name references across the codebase should use these constants
//! to avoid string literal scattering and enable easy renaming.

// File I/O
pub const READ: &str = "Read";
pub const EDIT: &str = "Edit";
pub const WRITE: &str = "Write";
pub const NOTEBOOK_EDIT: &str = "NotebookEdit";

// Execution
pub const BASH: &str = "Bash";

// Search
pub const GLOB: &str = "Glob";
pub const GREP: &str = "Grep";

// Web
pub const WEB_FETCH: &str = "WebFetch";
pub const WEB_SEARCH: &str = "WebSearch";

// Multi-Agent
pub const AGENT: &str = "Agent";
pub const TEAM_CREATE: &str = "TeamCreate";
pub const TEAM_DELETE: &str = "TeamDelete";
pub const SEND_MESSAGE: &str = "SendMessage";

// Tasks
pub const TASK_CREATE: &str = "TaskCreate";
pub const TASK_GET: &str = "TaskGet";
pub const TASK_LIST: &str = "TaskList";
pub const TASK_UPDATE: &str = "TaskUpdate";
pub const TASK_OUTPUT: &str = "TaskOutput";
pub const TASK_STOP: &str = "TaskStop";
pub const TODO_WRITE: &str = "TodoWrite";

// Planning
pub const ENTER_PLAN_MODE: &str = "EnterPlanMode";
pub const EXIT_PLAN_MODE: &str = "ExitPlanMode";

// Worktree
pub const ENTER_WORKTREE: &str = "EnterWorktree";
pub const EXIT_WORKTREE: &str = "ExitWorktree";

// Scheduling
pub const CRON_CREATE: &str = "CronCreate";
pub const CRON_DELETE: &str = "CronDelete";
pub const CRON_LIST: &str = "CronList";

// Meta
pub const SKILL: &str = "Skill";
pub const TOOL_SEARCH: &str = "ToolSearch";
pub const ASK_USER_QUESTION: &str = "AskUserQuestion";
pub const LSP: &str = "LSP";
pub const MEMORY: &str = "memory";

/// Old-to-new name mappings for backward compatibility.
/// Used by the alias system during migration.
pub fn legacy_aliases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("read_file", READ),
        ("edit_file", EDIT),
        ("write_file", WRITE),
        ("notebook_edit", NOTEBOOK_EDIT),
        ("run_command", BASH),
        ("list_files", GLOB),
        ("grep", GREP),
        ("web_fetch", WEB_FETCH),
        ("web_search", WEB_SEARCH),
        ("spawn_subagent", AGENT),
        ("create_team", TEAM_CREATE),
        ("delete_team", TEAM_DELETE),
        ("send_message", SEND_MESSAGE),
        ("ask_user", ASK_USER_QUESTION),
        ("invoke_skill", SKILL),
        ("lsp_query", LSP),
        ("task_complete", TASK_STOP),
        ("write_todos", TODO_WRITE),
        ("update_todo", TASK_UPDATE),
        ("list_todos", TASK_LIST),
        ("present_plan", ENTER_PLAN_MODE),
    ]
}
