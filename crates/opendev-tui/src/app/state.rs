//! Persistent application state shared across renders.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::history::CommandHistory;
use crate::widgets::{TaskWatcherFocus, TodoDisplayItem, WelcomePanelState};

use super::{AutonomyLevel, DisplayMessage, OperationMode, ReasoningLevel, ToolExecution};

/// Persistent application state shared across renders.
#[derive(Debug)]
pub struct AppState {
    /// Whether the app is running.
    pub running: bool,
    /// Current operation mode.
    pub mode: OperationMode,
    /// Autonomy level (Manual / Semi-Auto / Auto).
    pub autonomy: AutonomyLevel,
    /// Reasoning effort level (Off / Low / Medium / High).
    pub reasoning_level: ReasoningLevel,
    /// Active model name.
    pub model: String,
    /// Current working directory.
    pub working_dir: String,
    /// Git branch name (if in a repo).
    pub git_branch: Option<String>,
    /// Tokens used in current session.
    pub tokens_used: u64,
    /// Token limit for the session.
    pub tokens_limit: u64,
    /// Context window usage percentage (0.0 - 100.0+).
    pub context_usage_pct: f64,
    /// Session cost in USD.
    pub session_cost: f64,
    /// MCP server status: (connected, total).
    pub mcp_status: Option<(usize, usize)>,
    /// Whether any MCP server has errors.
    pub mcp_has_errors: bool,
    /// Whether the agent is currently processing.
    pub agent_active: bool,
    /// Conversation messages for display.
    pub messages: Vec<DisplayMessage>,
    /// Current task progress (while agent is working).
    pub task_progress: Option<crate::widgets::progress::TaskProgress>,
    /// Spinner state for animation.
    pub spinner: crate::widgets::spinner::SpinnerState,
    /// Current user input buffer.
    pub input_buffer: String,
    /// Cursor position within the input buffer.
    pub input_cursor: usize,
    /// Active tool executions.
    pub active_tools: Vec<ToolExecution>,
    /// Scroll offset for the conversation view.
    pub scroll_offset: u16,
    /// Whether the user has scrolled up (disables auto-scroll).
    pub user_scrolled: bool,
    /// Autocomplete engine for `/` commands and `@` file mentions.
    pub autocomplete: crate::autocomplete::AutocompleteEngine,
    /// Number of running background tasks.
    pub background_task_count: usize,
    /// Whether the background task panel overlay is open.
    pub background_panel_open: bool,
    /// Active subagent executions for nested display.
    pub active_subagents: Vec<crate::widgets::nested_tool::SubagentDisplayState>,
    /// Shared todo manager for syncing panel state with tool results.
    pub todo_manager: Option<Arc<Mutex<opendev_runtime::TodoManager>>>,
    /// Todo items from the current plan (for the todo progress panel).
    pub todo_items: Vec<TodoDisplayItem>,
    /// Whether the todo panel is expanded (true) or collapsed (false).
    pub todo_expanded: bool,
    /// Spinner tick counter for todo panel animation.
    pub todo_spinner_tick: usize,
    /// Optional plan name for the todo panel title.
    pub plan_name: Option<String>,
    /// File change stats for current session: (files, additions, deletions).
    pub file_changes: Option<(usize, u64, u64)>,
    /// Application version string.
    pub version: String,
    /// Animated welcome panel state.
    pub welcome_panel: WelcomePanelState,
    /// Cached terminal width for tick-time access.
    pub terminal_width: u16,
    /// Cached terminal height for tick-time access.
    pub terminal_height: u16,
    /// Queued messages submitted while agent was processing.
    pub pending_messages: Vec<String>,
    /// Dirty flag — set to `true` when state changes; cleared after render.
    pub dirty: bool,
    /// Generation counter for message/tool state changes.
    /// Incremented whenever messages, tool results, or collapse state change.
    pub message_generation: u64,
    /// Cached conversation lines (static message portion only, excludes spinners).
    pub cached_lines: Vec<ratatui::text::Line<'static>>,
    /// Generation counter at which `cached_lines` was last built.
    pub lines_generation: u64,
    /// Per-message content hashes for incremental cache rebuilds.
    pub per_message_hashes: Vec<u64>,
    /// Per-message line counts tracking how many cached_lines each message produced.
    pub per_message_line_counts: Vec<usize>,
    /// Per-message markdown render cache, keyed by hash of (role + content).
    pub markdown_cache: HashMap<u64, Vec<ratatui::text::Line<'static>>>,
    /// Terminal width at which cached_lines were last built (for resize invalidation).
    pub cached_width: u16,
    /// Scroll acceleration: last scroll direction (true = up, false = down).
    pub scroll_last_direction: Option<bool>,
    /// Scroll acceleration: timestamp of the last scroll key press.
    pub scroll_last_time: Option<Instant>,
    /// Scroll acceleration: current acceleration level (0 = base, increases).
    pub scroll_accel_level: u8,
    /// Active color theme for the TUI.
    pub theme: crate::formatters::style_tokens::Theme,
    /// Name of the active theme.
    pub theme_name: crate::formatters::style_tokens::ThemeName,
    /// Command history for Up/Down arrow navigation.
    pub command_history: CommandHistory,
    /// Flag set by /compact command; agent loop consumes and triggers compaction.
    pub compact_requested: bool,
    /// Whether manual compaction is currently in progress.
    pub compaction_active: bool,
    /// Plan mode flag — when true, next UserSubmit injects plan reminder.
    pub pending_plan_request: bool,
    /// Plan content to display in the conversation (consumed after first render).
    pub plan_content_display: Option<String>,
    /// Whether we're waiting for the current tool to finish before backgrounding.
    pub backgrounding_pending: bool,
    /// Background agent task manager.
    pub bg_agent_manager: crate::managers::BackgroundAgentManager,
    /// Whether the task watcher panel (Alt+B) is open.
    pub task_watcher_open: bool,
    /// Selected index in the task watcher list.
    pub task_watcher_selected: usize,
    /// Output scroll offset in the task watcher detail pane.
    pub task_watcher_output_scroll: u16,
    /// Which pane has focus in the task watcher.
    pub task_watcher_focus: TaskWatcherFocus,
    /// Last task completion flash: (task_id, when).
    pub last_task_completion: Option<(String, Instant)>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            running: true,
            mode: OperationMode::Normal,
            autonomy: AutonomyLevel::Manual,
            reasoning_level: ReasoningLevel::Medium,
            model: String::from("claude-sonnet-4"),
            working_dir: String::from("."),
            git_branch: None,
            tokens_used: 0,
            tokens_limit: 200_000,
            context_usage_pct: 0.0,
            session_cost: 0.0,
            mcp_status: None,
            mcp_has_errors: false,
            agent_active: false,
            messages: Vec::new(),
            task_progress: None,
            spinner: crate::widgets::spinner::SpinnerState::new(),
            input_buffer: String::new(),
            input_cursor: 0,
            active_tools: Vec::new(),
            scroll_offset: 0,
            user_scrolled: false,
            autocomplete: crate::autocomplete::AutocompleteEngine::new(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            ),
            background_task_count: 0,
            background_panel_open: false,
            active_subagents: Vec::new(),
            todo_manager: None,
            todo_items: Vec::new(),
            todo_expanded: true,
            todo_spinner_tick: 0,
            plan_name: None,
            file_changes: None,
            version: String::from("0.1.0"),
            welcome_panel: WelcomePanelState::new(),
            terminal_width: 80,
            terminal_height: 24,
            pending_messages: Vec::new(),
            dirty: true,
            message_generation: 0,
            cached_lines: Vec::new(),
            lines_generation: u64::MAX, // Force initial build
            per_message_hashes: Vec::new(),
            per_message_line_counts: Vec::new(),
            markdown_cache: HashMap::new(),
            cached_width: 80,
            scroll_last_direction: None,
            scroll_last_time: None,
            scroll_accel_level: 0,
            theme: crate::formatters::style_tokens::Theme::dark(),
            theme_name: crate::formatters::style_tokens::ThemeName::Dark,
            command_history: CommandHistory::new(),
            compact_requested: false,
            compaction_active: false,
            pending_plan_request: false,
            plan_content_display: None,
            backgrounding_pending: false,
            bg_agent_manager: crate::managers::BackgroundAgentManager::new(),
            task_watcher_open: false,
            task_watcher_selected: 0,
            task_watcher_output_scroll: 0,
            task_watcher_focus: TaskWatcherFocus::List,
            last_task_completion: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert!(state.running);
        assert_eq!(state.mode, OperationMode::Normal);
        assert!(state.messages.is_empty());
        assert!(state.input_buffer.is_empty());
    }

    #[test]
    fn test_dirty_flag_default() {
        let state = AppState::default();
        assert!(
            state.dirty,
            "AppState should start dirty for initial render"
        );
    }
}
