//! Main TUI application struct and event loop.
//!
//! Mirrors the Python `SWECLIChatApp` — manages terminal setup/teardown,
//! the main render loop, and dispatches events to widgets and controllers.

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::controllers::{ApprovalController, MessageController, PlanApprovalController};
use crate::event::{AppEvent, EventHandler};
use crate::history::CommandHistory;
use crate::widgets::{
    ConversationWidget, InputWidget, NestedToolWidget, StatusBarWidget, TodoDisplayItem,
    TodoDisplayStatus, TodoPanelWidget, WelcomePanelState, WelcomePanelWidget,
};
use crossterm::{
    event::{
        KeyCode, KeyModifiers, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, layout};
use tokio::sync::mpsc;

/// Operation mode — mirrors `OperationMode` from the Python side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationMode {
    Normal,
    Plan,
}

impl std::fmt::Display for OperationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::Plan => write!(f, "Plan"),
        }
    }
}

/// Autonomy level — mirrors Python `StatusBar.autonomy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomyLevel {
    Manual,
    SemiAuto,
    Auto,
}

impl std::fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manual => write!(f, "Manual"),
            Self::SemiAuto => write!(f, "Semi-Auto"),
            Self::Auto => write!(f, "Auto"),
        }
    }
}

/// Re-export ThinkingLevel from opendev-runtime for convenience.
pub use opendev_runtime::ThinkingLevel;

/// Persistent application state shared across renders.
#[derive(Debug)]
pub struct AppState {
    /// Whether the app is running.
    pub running: bool,
    /// Current operation mode.
    pub mode: OperationMode,
    /// Autonomy level (Manual / Semi-Auto / Auto).
    pub autonomy: AutonomyLevel,
    /// Thinking level (Off / Low / Medium / High).
    pub thinking_level: ThinkingLevel,
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
    /// Thinking trace blocks for the current turn.
    pub thinking_blocks: Vec<crate::widgets::thinking::ThinkingBlock>,
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
    /// Plan mode flag — when true, next UserSubmit injects plan reminder.
    pub pending_plan_request: bool,
    /// Plan content to display in the conversation (consumed after first render).
    pub plan_content_display: Option<String>,
}

/// Compute a hash key for markdown cache lookup from role and content.
fn markdown_cache_key(role: &DisplayRole, content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    std::mem::discriminant(role).hash(&mut hasher);
    content.hash(&mut hasher);
    hasher.finish()
}

/// Compute a content hash for a `DisplayMessage` used by per-message dirty tracking.
fn display_message_hash(msg: &DisplayMessage) -> u64 {
    let mut hasher = DefaultHasher::new();
    std::mem::discriminant(&msg.role).hash(&mut hasher);
    msg.content.hash(&mut hasher);
    msg.collapsed.hash(&mut hasher);
    if let Some(ref tc) = msg.tool_call {
        tc.name.hash(&mut hasher);
        format!("{:?}", tc.arguments).hash(&mut hasher);
        tc.summary.hash(&mut hasher);
        tc.success.hash(&mut hasher);
        tc.collapsed.hash(&mut hasher);
        tc.result_lines.hash(&mut hasher);
        tc.nested_calls.len().hash(&mut hasher);
        for nested in &tc.nested_calls {
            nested.name.hash(&mut hasher);
            nested.success.hash(&mut hasher);
            format!("{:?}", nested.arguments).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// A message prepared for display in the conversation widget.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: DisplayRole,
    pub content: String,
    /// Optional tool call info for assistant messages.
    pub tool_call: Option<DisplayToolCall>,
    /// Whether this message is collapsed (used for Thinking role).
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayRole {
    User,
    Assistant,
    System,
    Thinking,
    /// Interrupted feedback — rendered with ⎿ in red.
    Interrupt,
}

/// Tool call display info.
#[derive(Debug, Clone)]
pub struct DisplayToolCall {
    pub name: String,
    pub arguments: std::collections::HashMap<String, serde_json::Value>,
    pub summary: Option<String>,
    pub success: bool,
    /// Whether this tool result is collapsed (user can toggle).
    pub collapsed: bool,
    /// Result lines for expanded view.
    pub result_lines: Vec<String>,
    /// Nested tool calls (from subagent execution).
    pub nested_calls: Vec<DisplayToolCall>,
}

/// State of a tool execution lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolState {
    /// Tool is queued but not yet executing.
    Pending,
    /// Tool is currently executing.
    Running,
    /// Tool finished successfully.
    Completed,
    /// Tool finished with an error.
    Error,
    /// Tool was cancelled before completion.
    Cancelled,
}

impl ToolState {
    /// Returns true if the tool is in a terminal state (Completed, Error, or Cancelled).
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Completed | Self::Error | Self::Cancelled)
    }

    /// Returns true if the tool completed successfully.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Completed)
    }
}

/// Active tool execution being displayed.
#[derive(Debug, Clone)]
pub struct ToolExecution {
    pub id: String,
    pub name: String,
    pub output_lines: Vec<String>,
    /// Current state of the tool execution.
    pub state: ToolState,
    /// Elapsed seconds since tool started.
    pub elapsed_secs: u64,
    /// Start timestamp for elapsed time calculation.
    pub started_at: std::time::Instant,
    /// Animation frame counter — incremented every tick for smooth spinner.
    pub tick_count: usize,
    /// Parent tool ID for nested tool calls.
    pub parent_id: Option<String>,
    /// Nesting depth (0 = top-level).
    pub depth: usize,
    /// Tool arguments for display.
    pub args: std::collections::HashMap<String, serde_json::Value>,
}

impl ToolExecution {
    /// Whether the tool execution has finished (terminal state).
    pub fn is_finished(&self) -> bool {
        self.state.is_finished()
    }

    /// Whether the tool execution was successful.
    pub fn is_success(&self) -> bool {
        self.state.is_success()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            running: true,
            mode: OperationMode::Normal,
            autonomy: AutonomyLevel::Manual,
            thinking_level: ThinkingLevel::Medium,
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
            thinking_blocks: Vec::new(),
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
            active_subagents: Vec::new(),
            todo_manager: None,
            todo_items: Vec::new(),
            todo_expanded: true,
            todo_spinner_tick: 0,
            plan_name: None,
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
            scroll_last_direction: None,
            scroll_last_time: None,
            scroll_accel_level: 0,
            theme: crate::formatters::style_tokens::Theme::dark(),
            theme_name: crate::formatters::style_tokens::ThemeName::Dark,
            command_history: CommandHistory::new(),
            compact_requested: false,
            pending_plan_request: false,
            plan_content_display: None,
        }
    }
}

/// The main TUI application.
pub struct App {
    /// Application state.
    pub state: AppState,
    /// Event handler for terminal + agent events.
    event_handler: EventHandler,
    /// Channel for sending events back into the loop (e.g., from key handlers).
    event_tx: mpsc::UnboundedSender<AppEvent>,
    /// Message controller for handling user submissions.
    message_controller: MessageController,
    /// Approval controller for inline command approval prompts.
    approval_controller: ApprovalController,
    /// Plan approval controller for plan review prompts.
    plan_approval_controller: PlanApprovalController,
    /// Oneshot sender to forward the plan decision back to the tool.
    plan_approval_response_tx: Option<tokio::sync::oneshot::Sender<opendev_runtime::PlanDecision>>,
    /// Interrupt token for signaling cancellation to the agent (set per-query).
    interrupt_token: Option<opendev_runtime::InterruptToken>,
    /// Optional channel for forwarding user messages to the agent backend.
    user_message_tx: Option<mpsc::UnboundedSender<String>>,
}

impl Default for App {
    fn default() -> Self {
        App::new()
    }
}

impl App {
    /// Create a new TUI application with default state.
    pub fn new() -> Self {
        let event_handler = EventHandler::new(Duration::from_millis(60));
        let event_tx = event_handler.sender();
        Self {
            state: AppState::default(),
            event_handler,
            event_tx,
            message_controller: MessageController::new(),
            approval_controller: ApprovalController::new(),
            plan_approval_controller: PlanApprovalController::new(),
            plan_approval_response_tx: None,
            interrupt_token: None,
            user_message_tx: None,
        }
    }

    /// Attach a channel for forwarding user-submitted messages to the agent backend.
    ///
    /// When set, every `UserSubmit` event will also send the message text through
    /// this channel so the backend can process it.
    pub fn with_message_channel(mut self, tx: mpsc::UnboundedSender<String>) -> Self {
        self.user_message_tx = Some(tx);
        self
    }

    /// Get a sender for pushing events into the application loop.
    ///
    /// Agent and tool runners use this to notify the UI of state changes.
    pub fn event_sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.event_tx.clone()
    }

    /// Run the TUI application.
    ///
    /// Sets up the terminal, enters the event loop, and restores the
    /// terminal on exit or panic.
    pub async fn run(&mut self) -> io::Result<()> {
        // Terminal setup
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;

        // Enable xterm alternate scroll mode: terminal converts mouse wheel events
        // into Up/Down arrow key sequences. This gives us scroll support without
        // EnableMouseCapture, preserving native text selection.
        use std::io::Write;
        stdout.write_all(b"\x1b[?1007h")?;
        stdout.flush()?;

        // Enable Kitty keyboard protocol so terminals report Shift+Enter distinctly.
        // Always attempt to push the flags — unsupported terminals silently ignore the
        // escape sequence, and `supports_keyboard_enhancement()` is unreliable (it queries
        // the terminal and can timeout, returning false on terminals that DO support it).
        let keyboard_enhanced = execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )
        .is_ok();

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Start the event reader
        self.event_handler.start();

        // Main loop
        let result = self.event_loop(&mut terminal).await;

        // Terminal teardown (always runs)
        if keyboard_enhanced {
            let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
        }
        disable_raw_mode()?;
        // Disable xterm alternate scroll mode before leaving alternate screen
        {
            use std::io::Write;
            let _ = terminal.backend_mut().write_all(b"\x1b[?1007l");
        }
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    /// The core event loop: render -> wait for event -> drain queued events -> repeat.
    ///
    /// Draining all pending events before each render avoids redundant frames
    /// when typing fast (5 queued keys = 1 render instead of 5).
    /// The dirty flag skips renders when no state has changed.
    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> io::Result<()> {
        while self.state.running {
            // Cache terminal dimensions for tick-time access
            let size = terminal.size()?;
            self.state.terminal_width = size.width;
            self.state.terminal_height = size.height;

            // Only render when state has changed
            if self.state.dirty {
                // Rebuild cached conversation lines if message generation changed
                if self.state.lines_generation != self.state.message_generation {
                    self.rebuild_cached_lines();
                    self.state.lines_generation = self.state.message_generation;
                }

                terminal.draw(|frame| self.render(frame))?;
                self.state.dirty = false;
            }

            // Wait for at least one event
            if let Some(event) = self.event_handler.next().await {
                self.handle_event(event);
            }

            // Drain all remaining queued events before next render
            while let Some(event) = self.event_handler.try_next() {
                self.handle_event(event);
                if !self.state.running {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Clear the per-message markdown render cache.
    pub fn clear_markdown_cache(&mut self) {
        self.state.markdown_cache.clear();
    }

    /// Rebuild the cached static conversation lines from messages.
    ///
    /// Uses per-message dirty tracking: each message's content is hashed and
    /// compared with the stored hash. Only messages whose hash changed or that
    /// are new get re-rendered. If a message in the middle changed, we rebuild
    /// from that point forward.
    ///
    /// Viewport culling is still applied: messages far above the visible viewport
    /// emit placeholder blank lines to preserve scroll math.
    fn rebuild_cached_lines(&mut self) {
        use crate::formatters::display::strip_system_reminders;

        let num_messages = self.state.messages.len();

        // Compute per-message hashes for the current messages
        let new_hashes: Vec<u64> = self
            .state
            .messages
            .iter()
            .map(display_message_hash)
            .collect();

        // Find the first message index where the hash differs
        let first_dirty = {
            let old_len = self.state.per_message_hashes.len();
            if old_len > num_messages {
                0 // Messages were removed -- full rebuild
            } else {
                let mut dirty_idx = old_len;
                for (i, new_hash) in new_hashes
                    .iter()
                    .enumerate()
                    .take(old_len.min(num_messages))
                {
                    if self.state.per_message_hashes[i] != *new_hash {
                        dirty_idx = i;
                        break;
                    }
                }
                dirty_idx
            }
        };

        // Nothing changed
        if first_dirty >= num_messages && self.state.per_message_hashes.len() == num_messages {
            return;
        }

        // Truncate to the point before first_dirty
        let lines_to_keep: usize = self
            .state
            .per_message_line_counts
            .iter()
            .take(first_dirty)
            .sum();
        self.state.cached_lines.truncate(lines_to_keep);
        self.state.per_message_hashes.truncate(first_dirty);
        self.state.per_message_line_counts.truncate(first_dirty);

        // --- Viewport culling ---
        let viewport_h = self.state.terminal_height as usize;
        let buffer_lines = 50;
        let visible_from_bottom = self.state.scroll_offset as usize + viewport_h + buffer_lines;

        let msg_line_estimates: Vec<usize> = self
            .state
            .messages
            .iter()
            .map(|msg| {
                let content = strip_system_reminders(&msg.content);
                let text_lines = if content.is_empty() {
                    0
                } else {
                    content.lines().count()
                };
                let tool_lines = if let Some(ref tc) = msg.tool_call {
                    1 + if !tc.collapsed {
                        tc.result_lines.len()
                    } else if !tc.result_lines.is_empty() {
                        1
                    } else {
                        0
                    } + tc.nested_calls.len()
                } else {
                    0
                };
                text_lines + tool_lines + 1
            })
            .collect();

        let total_estimated: usize = msg_line_estimates.iter().sum();
        let cull_start = total_estimated.saturating_sub(visible_from_bottom);
        let mut cumulative = 0usize;
        let msg_visible: Vec<bool> = msg_line_estimates
            .iter()
            .map(|&est| {
                let msg_end = cumulative + est;
                cumulative = msg_end;
                msg_end > cull_start
            })
            .collect();

        // Re-render only messages from first_dirty onward
        for msg_idx in first_dirty..num_messages {
            let msg = &self.state.messages[msg_idx];
            let lines_before = self.state.cached_lines.len();

            if !msg_visible[msg_idx] {
                let est = msg_line_estimates[msg_idx];
                for _ in 0..est {
                    self.state.cached_lines.push(ratatui::text::Line::from(""));
                }
            } else {
                Self::render_single_message(
                    msg,
                    &mut self.state.cached_lines,
                    &mut self.state.markdown_cache,
                );
            }

            let lines_produced = self.state.cached_lines.len() - lines_before;
            self.state.per_message_hashes.push(new_hashes[msg_idx]);
            self.state.per_message_line_counts.push(lines_produced);
        }
    }

    /// Render a single `DisplayMessage` into styled lines, appending to `lines`.
    fn render_single_message(
        msg: &DisplayMessage,
        lines: &mut Vec<ratatui::text::Line<'static>>,
        markdown_cache: &mut HashMap<u64, Vec<ratatui::text::Line<'static>>>,
    ) {
        use crate::formatters::display::strip_system_reminders;
        use crate::formatters::markdown::MarkdownRenderer;
        use crate::formatters::style_tokens::{self, Indent};
        use crate::formatters::tool_registry::{
            categorize_tool, format_tool_call_display, tool_color,
        };
        use crate::widgets::spinner::{COMPLETED_CHAR, CONTINUATION_CHAR};
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};

        let content = strip_system_reminders(&msg.content);
        if content.is_empty() && msg.tool_call.is_none() {
            return;
        }

        match msg.role {
            DisplayRole::Assistant => {
                let cache_key = markdown_cache_key(&msg.role, &content);
                let md_lines = if let Some(cached) = markdown_cache.get(&cache_key) {
                    cached.clone()
                } else {
                    let rendered = MarkdownRenderer::render(&content);
                    markdown_cache.insert(cache_key, rendered.clone());
                    rendered
                };
                let mut leading_consumed = false;
                for md_line in md_lines {
                    let line_text: String = md_line
                        .spans
                        .iter()
                        .map(|s| s.content.to_string())
                        .collect();
                    let has_content = !line_text.trim().is_empty();

                    if !leading_consumed && has_content {
                        let mut spans = vec![Span::styled(
                            format!("{} ", COMPLETED_CHAR),
                            Style::default().fg(style_tokens::GREEN_BRIGHT),
                        )];
                        spans.extend(
                            md_line
                                .spans
                                .into_iter()
                                .map(|s| Span::styled(s.content.to_string(), s.style)),
                        );
                        lines.push(Line::from(spans));
                        leading_consumed = true;
                    } else {
                        let mut spans = vec![Span::raw(Indent::CONT)];
                        spans.extend(
                            md_line
                                .spans
                                .into_iter()
                                .map(|s| Span::styled(s.content.to_string(), s.style)),
                        );
                        lines.push(Line::from(spans));
                    }
                }
            }
            DisplayRole::User => {
                let content_lines: Vec<&str> = content.lines().collect();
                for (i, content_line) in content_lines.iter().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "> ".to_string(),
                                Style::default()
                                    .fg(style_tokens::ACCENT)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                content_line.to_string(),
                                Style::default().fg(style_tokens::PRIMARY),
                            ),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw(Indent::CONT),
                            Span::styled(
                                content_line.to_string(),
                                Style::default().fg(style_tokens::PRIMARY),
                            ),
                        ]));
                    }
                }
            }
            DisplayRole::System => {
                let content_lines: Vec<&str> = content.lines().collect();
                for (i, content_line) in content_lines.iter().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "! ".to_string(),
                                Style::default()
                                    .fg(style_tokens::WARNING)
                                    .add_modifier(Modifier::ITALIC),
                            ),
                            Span::styled(
                                content_line.to_string(),
                                Style::default().fg(style_tokens::SUBTLE),
                            ),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw(Indent::CONT),
                            Span::styled(
                                content_line.to_string(),
                                Style::default().fg(style_tokens::SUBTLE),
                            ),
                        ]));
                    }
                }
            }
            DisplayRole::Thinking => {
                if msg.collapsed {
                    let first = content.lines().next().unwrap_or("");
                    let count = content.lines().count();
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{} ", style_tokens::THINKING_ICON),
                            Style::default().fg(style_tokens::THINKING_BG),
                        ),
                        Span::styled(
                            format!("+ {first}... ({count} lines, Ctrl+O to expand)"),
                            Style::default()
                                .fg(style_tokens::THINKING_BG)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                } else {
                    for (i, content_line) in content.lines().enumerate() {
                        if i == 0 {
                            lines.push(Line::from(vec![
                                Span::styled(
                                    format!("{} ", style_tokens::THINKING_ICON),
                                    Style::default().fg(style_tokens::THINKING_BG),
                                ),
                                Span::styled(
                                    content_line.to_string(),
                                    Style::default()
                                        .fg(style_tokens::THINKING_BG)
                                        .add_modifier(Modifier::ITALIC),
                                ),
                            ]));
                        } else {
                            lines.push(Line::from(vec![
                                Span::raw(Indent::CONT),
                                Span::styled(
                                    content_line.to_string(),
                                    Style::default()
                                        .fg(style_tokens::THINKING_BG)
                                        .add_modifier(Modifier::ITALIC),
                                ),
                            ]));
                        }
                    }
                }
            }
        }

        // Tool call summary
        if let Some(ref tc) = msg.tool_call {
            let category = categorize_tool(&tc.name);
            let color = tool_color(category);
            let (icon, icon_color) = if tc.success {
                (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
            } else {
                (COMPLETED_CHAR, style_tokens::ERROR)
            };
            let display = format_tool_call_display(&tc.name, &tc.arguments);
            lines.push(Line::from(vec![
                Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
                Span::styled(
                    display,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ]));

            if !tc.collapsed && !tc.result_lines.is_empty() {
                for (i, result_line) in tc.result_lines.iter().enumerate() {
                    let prefix_char: Cow<'static, str> = if i == 0 {
                        format!("  {}  ", CONTINUATION_CHAR).into()
                    } else {
                        Cow::Borrowed(Indent::RESULT_CONT)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(prefix_char, Style::default().fg(style_tokens::SUBTLE)),
                        Span::styled(
                            result_line.clone(),
                            Style::default().fg(style_tokens::SUBTLE),
                        ),
                    ]));
                }
            } else if tc.collapsed && !tc.result_lines.is_empty() {
                let count = tc.result_lines.len();
                lines.push(Line::from(Span::styled(
                    format!(
                        "  {}  ({count} lines collapsed, press Ctrl+O to expand)",
                        CONTINUATION_CHAR
                    ),
                    Style::default()
                        .fg(style_tokens::SUBTLE)
                        .add_modifier(Modifier::ITALIC),
                )));
            }

            for nested in &tc.nested_calls {
                let n_category = categorize_tool(&nested.name);
                let n_color = tool_color(n_category);
                let (n_icon, n_icon_color) = if nested.success {
                    (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
                } else {
                    (COMPLETED_CHAR, style_tokens::ERROR)
                };
                let n_display = format_tool_call_display(&nested.name, &nested.arguments);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}\u{2514}\u{2500} ", Indent::CONT),
                        Style::default().fg(style_tokens::SUBTLE),
                    ),
                    Span::styled(format!("{n_icon} "), Style::default().fg(n_icon_color)),
                    Span::styled(n_display, Style::default().fg(n_color)),
                ]));
            }
        }

        // Blank line between messages
        lines.push(Line::from(""));
    }

    /// Render the full UI layout.
    fn render(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        // Layout: conversation (flexible) | todo panel (if active) | subagent display (if active)
        //         | input | status bar
        // Tool spinners and thinking progress are rendered inline in the conversation area.
        let has_subagents = !self.state.active_subagents.is_empty();
        let has_todos = !self.state.todo_items.is_empty();
        let todo_height: u16 = if has_todos {
            if self.state.todo_expanded {
                // 2 borders + 1 progress bar + items (capped at 12)
                (self.state.todo_items.len() as u16 + 3).min(12)
            } else {
                // Collapsed: border top + 1 line + border bottom
                3
            }
        } else {
            0
        };
        let subagent_height: u16 = if has_subagents {
            // Dynamic height: header + 2 lines per subagent + tool lines
            let lines: u16 = self
                .state
                .active_subagents
                .iter()
                .map(|s| 1 + s.active_tools.len() as u16 + s.completed_tools.len().min(3) as u16)
                .sum();
            (lines + 2).min(12) // Cap at 12 lines
        } else {
            0
        };

        let chunks = layout::Layout::default()
            .direction(layout::Direction::Vertical)
            .constraints(
                [
                    layout::Constraint::Min(5),                  // conversation
                    layout::Constraint::Length(todo_height),     // todo panel
                    layout::Constraint::Length(subagent_height), // subagent display
                    layout::Constraint::Length({
                        let input_lines = self.state.input_buffer.matches('\n').count() + 1;
                        (input_lines as u16 + 1).min(8) // +1 for separator, cap at 8
                    }), // input
                    layout::Constraint::Length(2),               // status bar
                ]
                .as_ref(),
            )
            .split(area);

        // Conversation
        let mode_str = match self.state.mode {
            OperationMode::Normal => "NORMAL",
            OperationMode::Plan => "PLAN",
        };

        // Show animated welcome panel when no messages (or during fade-out)
        if self.state.messages.is_empty() && !self.state.welcome_panel.fade_complete {
            let wp = WelcomePanelWidget::new(&self.state.welcome_panel)
                .version(&self.state.version)
                .mode(mode_str);
            frame.render_widget(wp, chunks[0]);
        } else {
            let mut conversation =
                ConversationWidget::new(&self.state.messages, self.state.scroll_offset)
                    .terminal_width(area.width)
                    .version(&self.state.version)
                    .working_dir(&self.state.working_dir)
                    .mode(mode_str)
                    .active_tools(&self.state.active_tools)
                    .task_progress(self.state.task_progress.as_ref())
                    .spinner_char(self.state.spinner.current());
            if !self.state.cached_lines.is_empty() {
                conversation = conversation.cached_lines(&self.state.cached_lines);
            }
            frame.render_widget(conversation, chunks[0]);
        }

        // Todo panel (only if plan has todos)
        if has_todos {
            let mut todo_widget = TodoPanelWidget::new(&self.state.todo_items)
                .with_expanded(self.state.todo_expanded)
                .with_spinner_tick(self.state.todo_spinner_tick);
            if let Some(ref name) = self.state.plan_name {
                todo_widget = todo_widget.with_plan_name(name);
            }
            frame.render_widget(todo_widget, chunks[1]);
        }

        // Subagent display (only if active)
        if has_subagents {
            let subagent_display = NestedToolWidget::new(&self.state.active_subagents);
            frame.render_widget(subagent_display, chunks[2]);
        }

        // Input
        let input = InputWidget::new(
            &self.state.input_buffer,
            self.state.input_cursor,
            mode_str,
            self.state.pending_messages.len(),
        );
        frame.render_widget(input, chunks[3]);

        // Autocomplete popup (rendered over conversation area)
        if self.state.autocomplete.is_visible() {
            self.render_autocomplete(frame, chunks[3]);
        }

        // Plan approval panel (rendered over input area when active)
        if self.plan_approval_controller.active() {
            self.render_plan_approval(frame, chunks[3]);
        }

        // Status bar
        let status = StatusBarWidget::new(
            &self.state.model,
            &self.state.working_dir,
            self.state.git_branch.as_deref(),
            self.state.tokens_used,
            self.state.tokens_limit,
            self.state.mode,
        )
        .autonomy(self.state.autonomy)
        .thinking_level(self.state.thinking_level)
        .context_usage_pct(self.state.context_usage_pct)
        .session_cost(self.state.session_cost)
        .mcp_status(self.state.mcp_status, self.state.mcp_has_errors)
        .background_tasks(self.state.background_task_count);
        frame.render_widget(status, chunks[4]);
    }

    /// Render autocomplete popup above the input area.
    fn render_autocomplete(&self, frame: &mut ratatui::Frame, input_area: layout::Rect) {
        use crate::autocomplete::CompletionKind;
        use crate::formatters::style_tokens;
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Paragraph};

        let items = self.state.autocomplete.items();
        let selected_idx = self.state.autocomplete.selected_index();
        let max_show = items.len().min(10);
        let popup_height = max_show as u16 + 2; // +2 for borders

        // Determine title and width based on completion kind
        let is_file_mode = items
            .first()
            .is_some_and(|i| i.kind == CompletionKind::File);
        let popup_width = if is_file_mode { 60 } else { 50 };
        let title = if is_file_mode {
            " Files "
        } else {
            " Commands "
        };

        let popup_area = layout::Rect {
            x: input_area.x,
            y: input_area.y.saturating_sub(popup_height),
            width: input_area.width.min(popup_width),
            height: popup_height,
        };

        let lines: Vec<Line> = items
            .iter()
            .take(max_show)
            .enumerate()
            .map(|(i, item)| {
                let selected = i == selected_idx;
                let (left, right) =
                    crate::autocomplete::formatters::CompletionFormatter::format(item);

                let label_style = if selected {
                    Style::default()
                        .fg(style_tokens::CODE_BG)
                        .bg(style_tokens::CYAN)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(style_tokens::PRIMARY)
                };
                let desc_style = if selected {
                    Style::default()
                        .fg(style_tokens::CODE_BG)
                        .bg(style_tokens::CYAN)
                } else {
                    Style::default().fg(style_tokens::SUBTLE)
                };

                Line::from(vec![
                    Span::styled(format!("  {left}"), label_style),
                    Span::styled(format!(" {right}"), desc_style),
                ])
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(style_tokens::BORDER))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(style_tokens::CYAN)
                    .add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(paragraph, popup_area);
    }

    /// Render the plan approval panel above the input area.
    fn render_plan_approval(&self, frame: &mut ratatui::Frame, input_area: layout::Rect) {
        use crate::formatters::style_tokens;
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Paragraph};

        let options = self.plan_approval_controller.options();
        let selected = self.plan_approval_controller.selected_action();

        // Build lines: header hint + options
        let mut lines = vec![Line::from(Span::styled(
            " \u{2191}/\u{2193} choose \u{00b7} Enter confirm \u{00b7} Esc cancel",
            Style::default().fg(style_tokens::SUBTLE),
        ))];

        for (i, opt) in options.iter().enumerate() {
            let is_selected = i == selected;
            let pointer = if is_selected { "\u{25b8}" } else { " " };
            let label_style = if is_selected {
                Style::default()
                    .fg(style_tokens::CYAN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(style_tokens::PRIMARY)
            };
            let desc_style = if is_selected {
                Style::default()
                    .fg(style_tokens::CODE_BG)
                    .bg(style_tokens::CYAN)
            } else {
                Style::default().fg(style_tokens::SUBTLE)
            };

            lines.push(Line::from(vec![
                Span::styled(format!("  {pointer} {}. {}", i + 1, opt.label), label_style),
                Span::styled(format!("  {}", opt.description), desc_style),
            ]));
        }

        // Panel height: 1 hint + options + 2 borders
        let panel_height = (lines.len() as u16 + 2).min(input_area.y);
        let popup_area = layout::Rect {
            x: input_area.x,
            y: input_area.y.saturating_sub(panel_height),
            width: input_area.width,
            height: panel_height,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(style_tokens::CYAN))
            .title(Span::styled(
                " Plan \u{00b7} Ready for review ",
                Style::default()
                    .fg(style_tokens::CYAN)
                    .add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(paragraph, popup_area);
    }

    /// Drain the next pending message from the queue.
    /// Displays the user message and sends it to the agent backend.
    fn drain_next_pending_message(&mut self) {
        if let Some(queued_msg) = self.state.pending_messages.first().cloned() {
            self.state.pending_messages.remove(0);
            // Display the user message NOW (deferred from queue time)
            self.message_controller
                .handle_user_submit(&mut self.state, &queued_msg);
            self.state.message_generation += 1;
            // Send to agent backend
            self.state.agent_active = true;
            let _ = self.event_tx.send(AppEvent::UserSubmit(queued_msg));
            self.state.dirty = true;
        }
    }

    /// Dispatch an event to the appropriate handler.
    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => {
                self.handle_key(key);
                self.state.dirty = true;
            }
            AppEvent::Resize(_, _) => {
                // ratatui handles resize automatically, but we need to re-render
                self.state.dirty = true;
            }
            AppEvent::Tick => {
                self.handle_tick();
                // Tick is dirty only when there are animations running
                if self.state.agent_active
                    || !self.state.active_tools.is_empty()
                    || !self.state.active_subagents.is_empty()
                    || self.state.task_progress.is_some()
                    || !self.state.welcome_panel.fade_complete
                {
                    self.state.dirty = true;
                }
            }

            // Budget events
            AppEvent::BudgetExhausted {
                cost_usd,
                budget_usd,
            } => {
                self.state.agent_active = false;
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!(
                        "Session cost budget exhausted: ${:.4} spent of ${:.2} budget. \
                         Agent paused. Use /budget to adjust.",
                        cost_usd, budget_usd
                    ),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.dirty = true;
                self.state.message_generation += 1;
            }

            // Context usage events
            AppEvent::ContextUsage(pct) => {
                self.state.context_usage_pct = pct;
                self.state.dirty = true;
            }

            // Agent events
            AppEvent::AgentStarted => {
                self.state.agent_active = true;
                self.state.dirty = true;
            }
            AppEvent::AgentChunk(text) => {
                self.message_controller
                    .handle_agent_chunk(&mut self.state, &text);
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::AgentMessage(msg) => {
                self.message_controller
                    .handle_agent_message(&mut self.state, msg);
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::AgentFinished => {
                self.state.agent_active = false;
                self.state.dirty = true;
                self.drain_next_pending_message();
            }
            AppEvent::AgentError(err) => {
                self.state.agent_active = false;
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("Error: {err}"),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.dirty = true;
                self.state.message_generation += 1;
                // Continue processing queued messages despite the error
                self.drain_next_pending_message();
            }

            // Thinking events
            AppEvent::ThinkingTrace(content) => {
                let auto_collapse = content.lines().count() > 5;
                // Replace previous thinking message if present (completion nudge
                // can trigger thinking phase again in the same agent turn).
                if let Some(last) = self.state.messages.last_mut()
                    && last.role == DisplayRole::Thinking
                {
                    last.collapsed = auto_collapse;
                    last.content = content;
                } else {
                    self.state.messages.push(DisplayMessage {
                        role: DisplayRole::Thinking,
                        content,
                        tool_call: None,
                        collapsed: auto_collapse,
                    });
                }
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::CritiqueTrace(content) => {
                let auto_collapse = content.lines().count() > 5;
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::Thinking,
                    content,
                    tool_call: None,
                    collapsed: auto_collapse,
                });
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::RefinedThinkingTrace(content) => {
                let auto_collapse = content.lines().count() > 5;
                // Replace previous thinking/critique if the refinement supersedes them
                if let Some(last) = self.state.messages.last_mut()
                    && last.role == DisplayRole::Thinking
                {
                    last.collapsed = auto_collapse;
                    last.content = content;
                } else {
                    self.state.messages.push(DisplayMessage {
                        role: DisplayRole::Thinking,
                        content,
                        tool_call: None,
                        collapsed: auto_collapse,
                    });
                }
                self.state.dirty = true;
                self.state.message_generation += 1;
            }

            // Tool events
            AppEvent::ToolStarted {
                tool_id,
                tool_name,
                args,
            } => {
                self.state.active_tools.push(ToolExecution {
                    id: tool_id,
                    name: tool_name,
                    output_lines: Vec::new(),
                    state: ToolState::Running,
                    elapsed_secs: 0,
                    started_at: std::time::Instant::now(),
                    tick_count: 0,
                    parent_id: None,
                    depth: 0,
                    args,
                });
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::ToolOutput { tool_id, output } => {
                if let Some(tool) = self.state.active_tools.iter_mut().find(|t| t.id == tool_id) {
                    tool.output_lines.push(output);
                }
                self.state.dirty = true;
            }
            AppEvent::ToolResult {
                tool_id,
                tool_name,
                output,
                success,
                args: result_args,
            } => {
                // Look up stored args from the ToolStarted event, fall back to result args
                let arguments = self
                    .state
                    .active_tools
                    .iter()
                    .find(|t| t.id == tool_id)
                    .map(|t| t.args.clone())
                    .unwrap_or(result_args);

                // Check if this is a todo tool for special handling
                let is_todo_tool = matches!(
                    tool_name.as_str(),
                    "write_todos" | "update_todo" | "complete_todo" | "list_todos" | "clear_todos"
                );

                let (display_lines, collapsed) = if is_todo_tool {
                    let summary = crate::formatters::todo_formatter::summarize_todo_result(
                        &tool_name, &output,
                    );
                    (vec![summary], false)
                } else {
                    let result_lines: Vec<String> =
                        output.lines().take(50).map(|l| l.to_string()).collect();
                    let lines = if result_lines.is_empty() && !output.is_empty() {
                        vec![output.clone()]
                    } else {
                        result_lines
                    };
                    let collapse = lines.len() > 5;
                    (lines, collapse)
                };

                if !display_lines.is_empty() {
                    self.state.messages.push(DisplayMessage {
                        role: DisplayRole::Assistant,
                        content: String::new(),
                        tool_call: Some(DisplayToolCall {
                            name: tool_name.clone(),
                            arguments,
                            summary: None,
                            success,
                            collapsed,
                            result_lines: display_lines,
                            nested_calls: Vec::new(),
                        }),
                        collapsed: false,
                    });
                }

                // Refresh todo panel from shared manager after any todo tool
                if is_todo_tool
                    && let Some(ref mgr) = self.state.todo_manager
                    && let Ok(mgr) = mgr.lock()
                {
                    self.state.todo_items = mgr
                        .all()
                        .iter()
                        .map(|item| TodoDisplayItem {
                            id: item.id,
                            title: item.title.clone(),
                            status: match item.status {
                                opendev_runtime::TodoStatus::Pending => TodoDisplayStatus::Pending,
                                opendev_runtime::TodoStatus::InProgress => {
                                    TodoDisplayStatus::InProgress
                                }
                                opendev_runtime::TodoStatus::Completed => {
                                    TodoDisplayStatus::Completed
                                }
                            },
                            active_form: if item.active_form.is_empty() {
                                None
                            } else {
                                Some(item.active_form.clone())
                            },
                        })
                        .collect();
                    if tool_name == "write_todos" && !self.state.todo_items.is_empty() {
                        self.state.todo_expanded = true;
                    }
                    if tool_name == "clear_todos" {
                        self.state.todo_items.clear();
                    }
                }

                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::ToolFinished { tool_id, success } => {
                if let Some(tool) = self.state.active_tools.iter_mut().find(|t| t.id == tool_id) {
                    tool.state = if success {
                        ToolState::Completed
                    } else {
                        ToolState::Error
                    };
                }
                // Remove finished tools after a brief display period
                self.state.active_tools.retain(|t| !t.is_finished());
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::ToolApprovalRequired {
                tool_id: _,
                tool_name: _,
                description,
            } => {
                // Activate the approval controller for this command.
                // The working directory comes from the app state.
                let wd = self.state.working_dir.clone();
                let _rx = self.approval_controller.start(description, wd);
                // The receiver will be consumed by the agent runner
                // via the event loop (Up/Down/Enter/Esc handled in handle_key).
                self.state.dirty = true;
            }

            // Subagent events
            AppEvent::SubagentStarted {
                subagent_name,
                task,
            } => {
                self.state.active_subagents.push(
                    crate::widgets::nested_tool::SubagentDisplayState::new(subagent_name, task),
                );
                self.state.dirty = true;
            }
            AppEvent::SubagentToolCall {
                subagent_name,
                tool_name,
                tool_id,
            } => {
                if let Some(subagent) = self
                    .state
                    .active_subagents
                    .iter_mut()
                    .find(|s| s.name == subagent_name && !s.finished)
                {
                    subagent.add_tool_call(tool_name, tool_id);
                }
                self.state.dirty = true;
            }
            AppEvent::SubagentToolComplete {
                subagent_name,
                tool_name: _,
                tool_id,
                success,
            } => {
                if let Some(subagent) = self
                    .state
                    .active_subagents
                    .iter_mut()
                    .find(|s| s.name == subagent_name && !s.finished)
                {
                    subagent.complete_tool_call(&tool_id, success);
                }
                self.state.dirty = true;
            }
            AppEvent::SubagentFinished {
                subagent_name,
                success,
                result_summary,
                tool_call_count,
                shallow_warning,
            } => {
                if let Some(subagent) = self
                    .state
                    .active_subagents
                    .iter_mut()
                    .find(|s| s.name == subagent_name && !s.finished)
                {
                    subagent.finish(success, result_summary, tool_call_count, shallow_warning);
                }
                // Remove finished subagents after marking them
                // (keep them for one more render so the user sees the result)
                self.state.dirty = true;
            }

            // Task progress events
            AppEvent::TaskProgressStarted { description } => {
                self.state.task_progress = Some(crate::widgets::progress::TaskProgress {
                    description,
                    elapsed_secs: 0,
                    token_display: None,
                    interrupted: false,
                    started_at: std::time::Instant::now(),
                });
                self.state.dirty = true;
            }
            AppEvent::TaskProgressFinished => {
                self.state.task_progress = None;
                self.state.dirty = true;
            }

            // UI events
            // Plan approval events
            AppEvent::PlanApprovalRequested {
                plan_content,
                response_tx,
            } => {
                // Store plan content for display in conversation
                self.state.plan_content_display = Some(plan_content.clone());
                // Add plan as a message in the conversation
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("── Plan ──\n{plan_content}"),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.message_generation += 1;
                // Start the plan approval controller
                let _rx = self.plan_approval_controller.start(plan_content);
                // Store the oneshot sender to forward the decision back to the tool
                self.plan_approval_response_tx = Some(response_tx);
                self.state.dirty = true;
            }

            AppEvent::UserSubmit(ref msg) => {
                // Forward to backend if channel is configured
                if let Some(ref tx) = self.user_message_tx {
                    let _ = tx.send(msg.clone());
                    self.state.agent_active = true;
                }
                self.state.dirty = true;
            }
            AppEvent::Interrupt => {
                // Cancel plan approval if active
                if self.plan_approval_controller.active() {
                    self.plan_approval_controller.cancel();
                    // cancel() sends "modify" via the controller's internal oneshot.
                    // Clean up our stored sender (already consumed by cancel).
                    self.plan_approval_response_tx.take();
                }
                if self.state.agent_active {
                    if let Some(ref token) = self.interrupt_token {
                        token.request(); // Signals all layers simultaneously
                    }
                    self.state.agent_active = false;
                    self.state.pending_messages.clear();
                }
                self.state.dirty = true;
            }
            AppEvent::SetInterruptToken(token) => {
                self.interrupt_token = Some(token);
            }
            AppEvent::AgentInterrupted => {
                self.state.agent_active = false;
                self.state.task_progress = None;
                // Clear active tools
                self.state.active_tools.clear();
                // Show interrupt feedback in the conversation
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::Interrupt,
                    content: "Interrupted".to_string(),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.dirty = true;
                self.state.message_generation += 1;
            }
            AppEvent::ModeChanged(mode) => {
                self.state.mode = match mode.as_str() {
                    "plan" => OperationMode::Plan,
                    _ => OperationMode::Normal,
                };
                self.state.dirty = true;
            }
            AppEvent::Quit => {
                self.state.running = false;
                self.state.dirty = true;
            }

            // Passthrough for unhandled events
            _ => {}
        }
    }

    /// Return the byte offset of the next char boundary after `pos` in `s`,
    /// or `s.len()` if already at the end.
    fn next_char_boundary(s: &str, pos: usize) -> usize {
        let mut idx = pos + 1;
        while idx < s.len() && !s.is_char_boundary(idx) {
            idx += 1;
        }
        idx.min(s.len())
    }

    /// Return the byte offset of the previous char boundary before `pos` in `s`,
    /// or 0 if already at the start.
    fn prev_char_boundary(s: &str, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        let mut idx = pos - 1;
        while idx > 0 && !s.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    /// Handle a key press event.
    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        // Only process key-press events (Kitty protocol also sends Release/Repeat)
        if key.kind != crossterm::event::KeyEventKind::Press {
            return;
        }

        // Delegate to plan approval controller when active
        if self.plan_approval_controller.active() {
            match key.code {
                KeyCode::Up => self.plan_approval_controller.prev(),
                KeyCode::Down => self.plan_approval_controller.next(),
                KeyCode::Enter => {
                    if let Some(decision) = self.plan_approval_controller.confirm() {
                        // Switch mode based on decision
                        match decision.action.as_str() {
                            "approve_auto" | "approve" => {
                                self.state.mode = OperationMode::Normal;
                            }
                            _ => {} // "modify" stays in Plan mode
                        }
                        // Forward decision back to the blocking tool
                        if let Some(tx) = self.plan_approval_response_tx.take() {
                            let _ = tx.send(decision);
                        }
                    }
                }
                KeyCode::Esc => {
                    self.plan_approval_controller.cancel();
                    // cancel() internally confirms with "modify" via the controller's
                    // oneshot — but we also need to forward through our stored sender.
                    // The controller already sent via its own oneshot in cancel(),
                    // so just clean up our stored tx (it's already consumed by cancel).
                    self.plan_approval_response_tx.take();
                }
                _ => {}
            }
            self.state.dirty = true;
            return;
        }

        // Delegate to approval controller when active
        if self.approval_controller.active() {
            match key.code {
                KeyCode::Up => self.approval_controller.move_selection(-1),
                KeyCode::Down => self.approval_controller.move_selection(1),
                KeyCode::Enter => self.approval_controller.confirm(),
                KeyCode::Esc => self.approval_controller.cancel(),
                _ => {}
            }
            return;
        }

        match (key.modifiers, key.code) {
            // Ctrl+C — quit or clear input
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                if self.state.input_buffer.is_empty() && !self.state.agent_active {
                    self.state.running = false;
                } else if self.state.agent_active {
                    // Interrupt agent
                    let _ = self.event_tx.send(AppEvent::Interrupt);
                } else {
                    self.state.input_buffer.clear();
                    self.state.input_cursor = 0;
                }
            }
            // Escape — dismiss autocomplete or interrupt agent
            (_, KeyCode::Esc) => {
                if self.state.autocomplete.is_visible() {
                    self.state.autocomplete.dismiss();
                } else {
                    let _ = self.event_tx.send(AppEvent::Interrupt);
                }
            }
            // Shift+Enter — insert newline in input buffer
            // iTerm2 (and many terminals) map Shift+Enter to Ctrl+J (ASCII LF).
            // Alt+Enter sends Enter with ALT modifier. Both insert a newline.
            (KeyModifiers::CONTROL, KeyCode::Char('j')) => {
                if !self.state.agent_active {
                    self.state
                        .input_buffer
                        .insert(self.state.input_cursor, '\n');
                    self.state.input_cursor += '\n'.len_utf8();
                }
            }
            (m, KeyCode::Enter)
                if m.contains(KeyModifiers::SHIFT) || m.contains(KeyModifiers::ALT) =>
            {
                if !self.state.agent_active {
                    self.state
                        .input_buffer
                        .insert(self.state.input_cursor, '\n');
                    self.state.input_cursor += '\n'.len_utf8();
                }
            }
            // Enter — accept autocomplete, submit message, or execute slash command
            (_, KeyCode::Enter) => {
                if self.state.autocomplete.is_visible() {
                    // Accept autocomplete selection
                    if let Some((insert_text, delete_count)) = self.state.autocomplete.accept() {
                        let start = self.state.input_cursor.saturating_sub(delete_count);
                        self.state
                            .input_buffer
                            .drain(start..self.state.input_cursor);
                        self.state.input_cursor = start;
                        self.state
                            .input_buffer
                            .insert_str(self.state.input_cursor, &insert_text);
                        self.state.input_cursor += insert_text.len();
                        // Add trailing space
                        self.state.input_buffer.insert(self.state.input_cursor, ' ');
                        self.state.input_cursor += 1;
                    }
                } else if !self.state.input_buffer.is_empty() {
                    let msg = self.state.input_buffer.clone();
                    self.state.input_buffer.clear();
                    self.state.input_cursor = 0;
                    self.state.autocomplete.dismiss();
                    self.state.command_history.record(&msg);

                    if self.state.agent_active {
                        // Queue silently — message will display when consumed
                        self.state.pending_messages.push(msg);
                        self.state.dirty = true;
                    } else {
                        // Start fading the welcome panel on first user message
                        if !self.state.welcome_panel.fade_complete
                            && !self.state.welcome_panel.is_fading
                        {
                            self.state.welcome_panel.start_fade();
                        }

                        if msg.starts_with('/') {
                            self.execute_slash_command(&msg);
                        } else {
                            self.message_controller
                                .handle_user_submit(&mut self.state, &msg);
                            self.state.message_generation += 1;
                            let _ = self.event_tx.send(AppEvent::UserSubmit(msg));
                        }
                    }
                }
            }
            // Backspace
            (_, KeyCode::Backspace) => {
                if self.state.input_cursor > 0 {
                    self.state.input_cursor =
                        Self::prev_char_boundary(&self.state.input_buffer, self.state.input_cursor);
                    self.state.input_buffer.remove(self.state.input_cursor);
                    self.update_autocomplete();
                }
            }
            // Delete
            (_, KeyCode::Delete) => {
                if self.state.input_cursor < self.state.input_buffer.len() {
                    self.state.input_buffer.remove(self.state.input_cursor);
                    self.update_autocomplete();
                }
            }
            // Left arrow
            (_, KeyCode::Left) => {
                if self.state.input_cursor > 0 {
                    self.state.input_cursor =
                        Self::prev_char_boundary(&self.state.input_buffer, self.state.input_cursor);
                }
            }
            // Right arrow
            (_, KeyCode::Right) => {
                if self.state.input_cursor < self.state.input_buffer.len() {
                    self.state.input_cursor =
                        Self::next_char_boundary(&self.state.input_buffer, self.state.input_cursor);
                }
            }
            // Home
            (_, KeyCode::Home) => {
                self.state.input_cursor = 0;
            }
            // End
            (_, KeyCode::End) => {
                self.state.input_cursor = self.state.input_buffer.len();
            }
            // Page Up — scroll conversation (with acceleration)
            (_, KeyCode::PageUp) => {
                let base = self.accelerated_scroll(true);
                // PageUp uses 3x the accelerated scroll amount
                let amount = base.saturating_mul(3);
                self.state.scroll_offset = self.state.scroll_offset.saturating_add(amount);
                self.state.user_scrolled = true;
            }
            // Page Down — scroll conversation (with acceleration)
            (_, KeyCode::PageDown) => {
                if self.state.scroll_offset > 0 {
                    let base = self.accelerated_scroll(false);
                    let amount = base.saturating_mul(3);
                    self.state.scroll_offset = self.state.scroll_offset.saturating_sub(amount);
                } else {
                    self.state.user_scrolled = false;
                }
            }
            // Shift+Tab — cycle mode and set pending plan flag
            (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                self.state.mode = match self.state.mode {
                    OperationMode::Normal => {
                        self.state.pending_plan_request = true;
                        OperationMode::Plan
                    }
                    OperationMode::Plan => {
                        self.state.pending_plan_request = false;
                        OperationMode::Normal
                    }
                };
            }
            // Ctrl+Shift+A — cycle autonomy level
            // Kitty keyboard protocol reports base key (lowercase 'a') with SHIFT modifier;
            // legacy terminals report uppercase 'A'. Handle both.
            (m, KeyCode::Char('A' | 'a'))
                if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) =>
            {
                self.state.autonomy = match self.state.autonomy {
                    AutonomyLevel::Manual => AutonomyLevel::SemiAuto,
                    AutonomyLevel::SemiAuto => AutonomyLevel::Auto,
                    AutonomyLevel::Auto => AutonomyLevel::Manual,
                };
            }
            // Ctrl+Shift+T — cycle thinking level
            // Same Kitty vs legacy handling as above.
            (m, KeyCode::Char('T' | 't'))
                if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) =>
            {
                self.state.thinking_level = match self.state.thinking_level {
                    ThinkingLevel::Off => ThinkingLevel::Low,
                    ThinkingLevel::Low => ThinkingLevel::Medium,
                    ThinkingLevel::Medium => ThinkingLevel::High,
                    ThinkingLevel::High => ThinkingLevel::Off,
                };
            }
            // Tab — accept autocomplete suggestion
            (_, KeyCode::Tab) => {
                if let Some((insert_text, delete_count)) = self.state.autocomplete.accept() {
                    let start = self.state.input_cursor.saturating_sub(delete_count);
                    self.state
                        .input_buffer
                        .drain(start..self.state.input_cursor);
                    self.state.input_cursor = start;
                    self.state
                        .input_buffer
                        .insert_str(self.state.input_cursor, &insert_text);
                    self.state.input_cursor += insert_text.len();
                    // Add trailing space
                    self.state.input_buffer.insert(self.state.input_cursor, ' ');
                    self.state.input_cursor += 1;
                }
            }
            // Up/Down arrow — autocomplete > command history > scroll
            (_, KeyCode::Up) => {
                if self.state.autocomplete.is_visible() {
                    self.state.autocomplete.select_prev();
                } else if !self.state.input_buffer.contains('\n') && !self.state.agent_active {
                    // Single-line input: navigate command history
                    if let Some(text) = self
                        .state
                        .command_history
                        .navigate_up(&self.state.input_buffer)
                    {
                        self.state.input_buffer = text.to_string();
                        self.state.input_cursor = self.state.input_buffer.len();
                    }
                } else {
                    let amount = self.accelerated_scroll(true);
                    self.state.scroll_offset = self.state.scroll_offset.saturating_add(amount);
                    self.state.user_scrolled = true;
                }
            }
            (_, KeyCode::Down) => {
                if self.state.autocomplete.is_visible() {
                    self.state.autocomplete.select_next();
                } else if self.state.command_history.is_navigating() {
                    // Navigate command history down
                    if let Some(text) = self.state.command_history.navigate_down() {
                        self.state.input_buffer = text.to_string();
                        self.state.input_cursor = self.state.input_buffer.len();
                    }
                } else if self.state.scroll_offset > 0 {
                    let amount = self.accelerated_scroll(false);
                    self.state.scroll_offset = self.state.scroll_offset.saturating_sub(amount);
                } else {
                    self.state.user_scrolled = false;
                }
            }
            // Ctrl+O — toggle collapsed state on the most recent collapsible tool result
            (KeyModifiers::CONTROL, KeyCode::Char('o')) => {
                // Priority 1: Most recent collapsible tool result
                let mut toggled = false;
                for msg in self.state.messages.iter_mut().rev() {
                    if let Some(ref mut tc) = msg.tool_call
                        && !tc.result_lines.is_empty()
                    {
                        tc.collapsed = !tc.collapsed;
                        self.state.message_generation += 1;
                        toggled = true;
                        break;
                    }
                }
                // Priority 2: Most recent thinking block
                if !toggled {
                    for msg in self.state.messages.iter_mut().rev() {
                        if msg.role == DisplayRole::Thinking && !msg.content.is_empty() {
                            msg.collapsed = !msg.collapsed;
                            self.state.message_generation += 1;
                            break;
                        }
                    }
                }
            }
            // Ctrl+T — toggle todo panel expanded/collapsed
            (KeyModifiers::CONTROL, KeyCode::Char('t')) => {
                if !self.state.todo_items.is_empty() {
                    self.state.todo_expanded = !self.state.todo_expanded;
                    self.state.dirty = true;
                }
            }
            // Ctrl+B — show background tasks info
            (KeyModifiers::CONTROL, KeyCode::Char('b')) => {
                let count = self.state.background_task_count;
                if count > 0 {
                    let task_word = if count == 1 { "task" } else { "tasks" };
                    self.state.messages.push(DisplayMessage {
                        role: DisplayRole::System,
                        content: format!("{count} background {task_word} running."),
                        tool_call: None,
                        collapsed: false,
                    });
                } else {
                    self.state.messages.push(DisplayMessage {
                        role: DisplayRole::System,
                        content: "No background tasks running.".to_string(),
                        tool_call: None,
                        collapsed: false,
                    });
                }
                self.state.message_generation += 1;
            }
            // Regular character input
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                self.state.command_history.reset_navigation();
                self.state.input_buffer.insert(self.state.input_cursor, c);
                self.state.input_cursor += c.len_utf8();
                // Update autocomplete on input change
                self.update_autocomplete();
            }
            _ => {}
        }
    }

    /// Compute the scroll amount with acceleration.
    ///
    /// If the same scroll direction is repeated within 200ms, the amount
    /// increases: 3 -> 6 -> 12 (then caps). Resets on direction change or timeout.
    fn accelerated_scroll(&mut self, up: bool) -> u16 {
        let now = Instant::now();
        let same_direction = self.state.scroll_last_direction == Some(up);
        let within_window = self
            .state
            .scroll_last_time
            .is_some_and(|t| now.duration_since(t) < Duration::from_millis(200));

        if same_direction && within_window {
            self.state.scroll_accel_level = (self.state.scroll_accel_level + 1).min(2);
        } else {
            self.state.scroll_accel_level = 0;
        }

        self.state.scroll_last_direction = Some(up);
        self.state.scroll_last_time = Some(now);

        match self.state.scroll_accel_level {
            0 => 3,
            1 => 6,
            _ => 12,
        }
    }

    /// Update autocomplete suggestions based on current input.
    fn update_autocomplete(&mut self) {
        if self.state.agent_active {
            self.state.autocomplete.dismiss();
            return;
        }
        let text_before_cursor = self.state.input_buffer[..self.state.input_cursor].to_string();
        self.state.autocomplete.update(&text_before_cursor);
    }

    /// Execute a slash command locally.
    fn execute_slash_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd[1..].splitn(2, ' ').collect();
        let name = parts[0];

        match name {
            "exit" | "quit" | "q" => {
                self.state.running = false;
            }
            "clear" => {
                self.state.messages.clear();
                self.state.scroll_offset = 0;
                self.state.user_scrolled = false;
                self.state.message_generation += 1;
            }
            "mode" => {
                self.state.mode = match self.state.mode {
                    OperationMode::Normal => OperationMode::Plan,
                    OperationMode::Plan => OperationMode::Normal,
                };
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("Mode: {}", self.state.mode),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.message_generation += 1;
            }
            "thinking" => {
                self.state.thinking_level = match self.state.thinking_level {
                    ThinkingLevel::Off => ThinkingLevel::Low,
                    ThinkingLevel::Low => ThinkingLevel::Medium,
                    ThinkingLevel::Medium => ThinkingLevel::High,
                    ThinkingLevel::High => ThinkingLevel::Off,
                };
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("Thinking: {}", self.state.thinking_level),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.message_generation += 1;
            }
            "autonomy" => {
                self.state.autonomy = match self.state.autonomy {
                    AutonomyLevel::Manual => AutonomyLevel::SemiAuto,
                    AutonomyLevel::SemiAuto => AutonomyLevel::Auto,
                    AutonomyLevel::Auto => AutonomyLevel::Manual,
                };
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("Autonomy: {}", self.state.autonomy),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.message_generation += 1;
            }
            "sound" => {
                opendev_runtime::play_finish_sound();
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: "Playing test sound...".to_string(),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.message_generation += 1;
            }
            "compact" => {
                if self.state.messages.len() < 5 {
                    self.state.messages.push(DisplayMessage {
                        role: DisplayRole::System,
                        content: "Not enough messages to compact (need at least 5).".to_string(),
                        tool_call: None,
                        collapsed: false,
                    });
                } else {
                    self.state.compact_requested = true;
                    self.state.messages.push(DisplayMessage {
                        role: DisplayRole::System,
                        content: "Context compaction triggered. Will compact on next query."
                            .to_string(),
                        tool_call: None,
                        collapsed: false,
                    });
                }
                self.state.message_generation += 1;
            }
            "help" => {
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: [
                        "Available commands:",
                        "  /help       — Show this help",
                        "  /clear      — Clear conversation",
                        "  /mode       — Toggle Normal/Plan mode",
                        "  /thinking   — Cycle thinking level",
                        "  /autonomy   — Cycle autonomy level",
                        "  /sound      — Play test notification sound",
                        "  /compact    — Compact conversation context",
                        "  /exit       — Quit OpenDev",
                        "",
                        "Keyboard shortcuts:",
                        "  Ctrl+C      — Clear input / interrupt / quit",
                        "  Escape      — Interrupt agent",
                        "  Shift+Tab   — Toggle mode",
                        "  PageUp/Down — Scroll conversation",
                    ]
                    .join("\n"),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.message_generation += 1;
            }
            _ => {
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!(
                        "Unknown command: /{name}. Type /help for available commands."
                    ),
                    tool_call: None,
                    collapsed: false,
                });
                self.state.message_generation += 1;
            }
        }
    }

    /// Handle periodic tick (spinner animation, etc.).
    fn handle_tick(&mut self) {
        // Advance welcome panel animation
        if !self.state.welcome_panel.fade_complete {
            // Ensure rain field is initialized/resized before ticking
            let w = self.state.terminal_width;
            let h = self.state.terminal_height;
            let rain_w = ((w as f32 * 0.7) as usize).clamp(20, 90);
            let rain_h = (h.saturating_sub(11) as usize).clamp(4, 20);
            self.state.welcome_panel.ensure_rain_field(rain_w, rain_h);
            self.state.welcome_panel.tick(w, h);
        }

        // Advance spinner animation
        if self.state.agent_active || !self.state.active_tools.is_empty() {
            self.state.spinner.tick();
        }

        // Advance todo spinner (for collapsed mode)
        if !self.state.todo_items.is_empty() {
            self.state.todo_spinner_tick = self.state.todo_spinner_tick.wrapping_add(1);
        }

        // Update elapsed time and tick counter on active tools
        for tool in &mut self.state.active_tools {
            if !tool.is_finished() {
                tool.elapsed_secs = tool.started_at.elapsed().as_secs();
                tool.tick_count += 1;
            }
        }

        // Animate active subagents and clean up finished ones
        for subagent in &mut self.state.active_subagents {
            if !subagent.finished {
                subagent.advance_tick();
            }
        }
        // Remove subagents that finished more than 3 seconds ago
        self.state
            .active_subagents
            .retain(|s| !s.finished || s.elapsed_secs() < 3);

        // Update task progress elapsed time from wall clock
        if let Some(ref mut progress) = self.state.task_progress {
            progress.elapsed_secs = progress.started_at.elapsed().as_secs();
        }

        // Auto-scroll if user hasn't manually scrolled up
        if !self.state.user_scrolled {
            self.state.scroll_offset = 0;
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
    fn test_operation_mode_display() {
        assert_eq!(OperationMode::Normal.to_string(), "Normal");
        assert_eq!(OperationMode::Plan.to_string(), "Plan");
    }

    #[test]
    fn test_app_creation() {
        let app = App::new();
        assert!(app.state.running);
        assert_eq!(app.state.mode, OperationMode::Normal);
    }

    #[test]
    fn test_handle_key_char_input() {
        let mut app = App::new();
        let key = crossterm::event::KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(key);
        assert_eq!(app.state.input_buffer, "a");
        assert_eq!(app.state.input_cursor, 1);
    }

    #[test]
    fn test_handle_key_backspace() {
        let mut app = App::new();
        app.state.input_buffer = "abc".into();
        app.state.input_cursor = 3;
        let key = crossterm::event::KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        app.handle_key(key);
        assert_eq!(app.state.input_buffer, "ab");
        assert_eq!(app.state.input_cursor, 2);
    }

    #[test]
    fn test_handle_key_enter_submits() {
        let mut app = App::new();
        app.state.input_buffer = "hello".into();
        app.state.input_cursor = 5;
        let key = crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(key);
        assert!(app.state.input_buffer.is_empty());
        assert_eq!(app.state.input_cursor, 0);
        // Should have added a user message
        assert_eq!(app.state.messages.len(), 1);
        assert_eq!(app.state.messages[0].role, DisplayRole::User);
    }

    #[test]
    fn test_mode_toggle() {
        let mut app = App::new();
        let key = crossterm::event::KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        app.handle_key(key);
        assert_eq!(app.state.mode, OperationMode::Plan);
        app.handle_key(key);
        assert_eq!(app.state.mode, OperationMode::Normal);
    }

    #[test]
    fn test_page_scroll() {
        let mut app = App::new();
        let pgup = crossterm::event::KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE);
        app.handle_key(pgup);
        // First press: base=3 (no accel), page multiplier 3x = 9
        assert_eq!(app.state.scroll_offset, 9);
        assert!(app.state.user_scrolled);

        // Page down reduces offset; direction change resets accel, so base=3, 3x=9
        let pgdn = crossterm::event::KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
        app.handle_key(pgdn);
        assert_eq!(app.state.scroll_offset, 0);
        // user_scrolled only clears when already at 0 and page down again
        assert!(app.state.user_scrolled);

        // One more page down at 0 clears user_scrolled
        app.handle_key(pgdn);
        assert!(!app.state.user_scrolled);
    }

    #[test]
    fn test_scroll_acceleration() {
        let mut app = App::new();
        // Set agent_active so Up/Down arrow scrolls (bypasses command history)
        app.state.agent_active = true;
        // First up-arrow: base amount = 3
        let up = crossterm::event::KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 3);
        assert_eq!(app.state.scroll_accel_level, 0);

        // Immediate second press (within 200ms): accelerates to 6
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 9); // 3 + 6
        assert_eq!(app.state.scroll_accel_level, 1);

        // Third press: accelerates to 12
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 21); // 9 + 12
        assert_eq!(app.state.scroll_accel_level, 2);

        // Fourth press: stays at 12 (capped)
        app.handle_key(up);
        assert_eq!(app.state.scroll_offset, 33); // 21 + 12
        assert_eq!(app.state.scroll_accel_level, 2);
    }

    #[test]
    fn test_scroll_acceleration_resets_on_direction_change() {
        let mut app = App::new();
        // Set agent_active so Up/Down arrow scrolls (bypasses command history)
        app.state.agent_active = true;
        let up = crossterm::event::KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let down = crossterm::event::KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);

        // Build up acceleration
        app.handle_key(up);
        app.handle_key(up);
        assert_eq!(app.state.scroll_accel_level, 1);
        assert_eq!(app.state.scroll_offset, 9); // 3 + 6

        // Direction change resets acceleration
        app.handle_key(down);
        assert_eq!(app.state.scroll_accel_level, 0);
        assert_eq!(app.state.scroll_offset, 6); // 9 - 3
    }

    #[test]
    fn test_dirty_flag_default() {
        let state = AppState::default();
        assert!(
            state.dirty,
            "AppState should start dirty for initial render"
        );
    }

    #[test]
    fn test_viewport_culling_cached_lines() {
        let mut app = App::new();
        // Add many messages
        for i in 0..100 {
            app.state.messages.push(DisplayMessage {
                role: DisplayRole::User,
                content: format!("Message {i}"),
                tool_call: None,
                collapsed: false,
            });
        }
        app.state.message_generation = 1;
        app.state.terminal_height = 24;
        app.state.scroll_offset = 0;

        // Build cached lines
        app.rebuild_cached_lines();

        // Should have lines for all messages (some may be placeholders)
        assert!(
            !app.state.cached_lines.is_empty(),
            "cached_lines should not be empty"
        );
    }

    // ---------------------------------------------------------------
    // Per-message dirty tracking tests
    // ---------------------------------------------------------------

    #[test]
    fn test_markdown_cache_hit() {
        let mut app = App::new();
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Assistant,
            content: "Hello **world**".into(),
            tool_call: None,
            collapsed: false,
        });
        app.state.terminal_height = 24;
        app.rebuild_cached_lines();
        assert_eq!(app.state.markdown_cache.len(), 1);
        let first_lines = app.state.cached_lines.clone();
        app.state.per_message_hashes.clear();
        app.state.per_message_line_counts.clear();
        app.state.cached_lines.clear();
        app.rebuild_cached_lines();
        assert_eq!(app.state.markdown_cache.len(), 1);
        assert_eq!(app.state.cached_lines.len(), first_lines.len());
    }

    #[test]
    fn test_markdown_cache_miss_different_content() {
        let mut app = App::new();
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Assistant,
            content: "Hello **world**".into(),
            tool_call: None,
            collapsed: false,
        });
        app.state.terminal_height = 24;
        app.rebuild_cached_lines();
        assert_eq!(app.state.markdown_cache.len(), 1);
        app.state.messages[0].content = "Goodbye **world**".into();
        app.rebuild_cached_lines();
        assert_eq!(app.state.markdown_cache.len(), 2);
    }

    #[test]
    fn test_markdown_cache_clear() {
        let mut app = App::new();
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Assistant,
            content: "# Title\nSome text".into(),
            tool_call: None,
            collapsed: false,
        });
        app.state.terminal_height = 24;
        app.rebuild_cached_lines();
        assert!(!app.state.markdown_cache.is_empty());
        app.clear_markdown_cache();
        assert!(app.state.markdown_cache.is_empty());
    }

    #[test]
    fn test_incremental_append_only_renders_new_message() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::User,
            content: "First message".into(),
            tool_call: None,
            collapsed: false,
        });
        app.rebuild_cached_lines();
        let lines_after_first = app.state.cached_lines.len();
        assert!(lines_after_first > 0);
        assert_eq!(app.state.per_message_hashes.len(), 1);
        assert_eq!(app.state.per_message_line_counts.len(), 1);
        let first_hash = app.state.per_message_hashes[0];
        let first_lines_snapshot = app.state.cached_lines.clone();

        // Append a second message
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::User,
            content: "Second message".into(),
            tool_call: None,
            collapsed: false,
        });
        app.rebuild_cached_lines();
        assert_eq!(
            app.state.per_message_hashes[0], first_hash,
            "first message hash should be unchanged after append"
        );
        assert_eq!(app.state.per_message_hashes.len(), 2);
        for i in 0..first_lines_snapshot.len() {
            assert_eq!(
                format!("{:?}", app.state.cached_lines[i]),
                format!("{:?}", first_lines_snapshot[i]),
                "first message lines should be preserved at index {i}"
            );
        }
        assert!(app.state.cached_lines.len() > lines_after_first);
    }

    #[test]
    fn test_incremental_modify_middle_rebuilds_from_change() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        for content in &["First", "Second", "Third"] {
            app.state.messages.push(DisplayMessage {
                role: DisplayRole::User,
                content: content.to_string(),
                tool_call: None,
                collapsed: false,
            });
        }
        app.rebuild_cached_lines();
        let original_lines = app.state.cached_lines.len();
        assert_eq!(app.state.per_message_hashes.len(), 3);
        let first_hash = app.state.per_message_hashes[0];
        let first_line_count = app.state.per_message_line_counts[0];

        // Modify the second message
        app.state.messages[1].content = "Modified Second".into();
        app.rebuild_cached_lines();

        // First message preserved
        assert_eq!(app.state.per_message_hashes[0], first_hash);
        assert_eq!(app.state.per_message_line_counts[0], first_line_count);
        assert_eq!(app.state.per_message_hashes.len(), 3);
        // Second hash changed
        assert_ne!(
            app.state.per_message_hashes[1],
            display_message_hash(&DisplayMessage {
                role: DisplayRole::User,
                content: "Second".into(),
                tool_call: None,
                collapsed: false,
            }),
        );
        assert_eq!(app.state.cached_lines.len(), original_lines);
    }

    #[test]
    fn test_incremental_empty_conversation() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        app.rebuild_cached_lines();
        assert!(app.state.cached_lines.is_empty());
        assert!(app.state.per_message_hashes.is_empty());
        assert!(app.state.per_message_line_counts.is_empty());
    }

    #[test]
    fn test_incremental_multiple_appends_correct_cache() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        for i in 0..5u32 {
            app.state.messages.push(DisplayMessage {
                role: if i % 2 == 0 {
                    DisplayRole::User
                } else {
                    DisplayRole::Assistant
                },
                content: format!("Message {i}"),
                tool_call: None,
                collapsed: false,
            });
            app.rebuild_cached_lines();
            assert_eq!(app.state.per_message_hashes.len(), (i + 1) as usize);
            assert_eq!(app.state.per_message_line_counts.len(), (i + 1) as usize);
        }
        // Compare with full rebuild
        let incremental_lines = app.state.cached_lines.clone();
        app.state.per_message_hashes.clear();
        app.state.per_message_line_counts.clear();
        app.state.cached_lines.clear();
        app.rebuild_cached_lines();
        assert_eq!(app.state.cached_lines.len(), incremental_lines.len());
        for (i, (inc, full)) in incremental_lines
            .iter()
            .zip(app.state.cached_lines.iter())
            .enumerate()
        {
            assert_eq!(
                format!("{:?}", inc),
                format!("{:?}", full),
                "line {i} differs between incremental and full rebuild"
            );
        }
    }

    #[test]
    fn test_incremental_no_change_is_noop() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::User,
            content: "Hello".into(),
            tool_call: None,
            collapsed: false,
        });
        app.rebuild_cached_lines();
        let lines_after = app.state.cached_lines.clone();
        // Second rebuild with no changes
        app.rebuild_cached_lines();
        assert_eq!(app.state.cached_lines.len(), lines_after.len());
    }

    #[test]
    fn test_incremental_message_removal() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::User,
            content: "First".into(),
            tool_call: None,
            collapsed: false,
        });
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::User,
            content: "Second".into(),
            tool_call: None,
            collapsed: false,
        });
        app.rebuild_cached_lines();
        assert_eq!(app.state.per_message_hashes.len(), 2);
        app.state.messages.pop();
        app.rebuild_cached_lines();
        assert_eq!(app.state.per_message_hashes.len(), 1);
        assert_eq!(app.state.per_message_line_counts.len(), 1);
    }
}
