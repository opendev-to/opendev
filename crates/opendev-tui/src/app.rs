//! Main TUI application struct and event loop.
//!
//! Mirrors the Python `SWECLIChatApp` — manages terminal setup/teardown,
//! the main render loop, and dispatches events to widgets and controllers.

use std::io;
use std::time::Duration;

use crate::controllers::{ApprovalController, MessageController};
use crate::event::{AppEvent, EventHandler};
use crate::managers::InterruptManager;
use crate::widgets::{
    ConversationWidget, InputWidget, NestedToolWidget, StatusBarWidget, TodoDisplayItem,
    TodoPanelWidget, ToolDisplayWidget, WelcomePanelState, WelcomePanelWidget,
};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode, KeyModifiers},
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
#[derive(Debug, Clone)]
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
    /// Autocomplete suggestions for slash commands.
    pub autocomplete_suggestions: Vec<&'static crate::controllers::slash_commands::SlashCommand>,
    /// Whether autocomplete popup is visible.
    pub autocomplete_visible: bool,
    /// Selected index in autocomplete list.
    pub autocomplete_index: usize,
    /// Number of running background tasks.
    pub background_task_count: usize,
    /// Active subagent executions for nested display.
    pub active_subagents: Vec<crate::widgets::nested_tool::SubagentDisplayState>,
    /// Todo items from the current plan (for the todo progress panel).
    pub todo_items: Vec<TodoDisplayItem>,
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
}

/// A message prepared for display in the conversation widget.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: DisplayRole,
    pub content: String,
    /// Optional tool call info for assistant messages.
    pub tool_call: Option<DisplayToolCall>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayRole {
    User,
    Assistant,
    System,
    Thinking,
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

/// Active tool execution being displayed.
#[derive(Debug, Clone)]
pub struct ToolExecution {
    pub id: String,
    pub name: String,
    pub output_lines: Vec<String>,
    pub finished: bool,
    pub success: bool,
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
            autocomplete_suggestions: Vec::new(),
            autocomplete_visible: false,
            autocomplete_index: 0,
            background_task_count: 0,
            active_subagents: Vec::new(),
            todo_items: Vec::new(),
            plan_name: None,
            version: String::from("0.1.0"),
            welcome_panel: WelcomePanelState::new(),
            terminal_width: 80,
            terminal_height: 24,
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
    /// Interrupt manager for signaling cancellation to the agent.
    interrupt_manager: InterruptManager,
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
            interrupt_manager: InterruptManager::new(),
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
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Start the event reader
        self.event_handler.start();

        // Main loop
        let result = self.event_loop(&mut terminal).await;

        // Terminal teardown (always runs)
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture,
        )?;
        terminal.show_cursor()?;

        result
    }

    /// The core event loop: render -> wait for event -> drain queued events -> repeat.
    ///
    /// Draining all pending events before each render avoids redundant frames
    /// when typing fast (5 queued keys = 1 render instead of 5).
    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> io::Result<()> {
        while self.state.running {
            // Cache terminal dimensions for tick-time access
            let size = terminal.size()?;
            self.state.terminal_width = size.width;
            self.state.terminal_height = size.height;

            // Render
            terminal.draw(|frame| self.render(frame))?;

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

    /// Render the full UI layout.
    fn render(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        // Layout: conversation (flexible) | todo panel (if active) | subagent display (if active)
        //         | tool display (if active) | progress (if active) | input (3 lines) | status bar (1 line)
        let has_tools = !self.state.active_tools.is_empty();
        let has_subagents = !self.state.active_subagents.is_empty();
        let has_todos = !self.state.todo_items.is_empty();
        let tool_height = if has_tools { 8 } else { 0 };
        let todo_height: u16 = if has_todos {
            // 2 borders + 1 progress bar + items (capped)
            (self.state.todo_items.len() as u16 + 3).min(10)
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
        let progress_height: u16 = if self.state.task_progress.is_some() {
            1
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
                    layout::Constraint::Length(tool_height),     // tool display
                    layout::Constraint::Length(progress_height), // task progress
                    layout::Constraint::Length(2),               // input
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
            let conversation =
                ConversationWidget::new(&self.state.messages, self.state.scroll_offset)
                    .terminal_width(area.width)
                    .version(&self.state.version)
                    .working_dir(&self.state.working_dir)
                    .mode(mode_str);
            frame.render_widget(conversation, chunks[0]);
        }

        // Todo panel (only if plan has todos)
        if has_todos {
            let mut todo_widget = TodoPanelWidget::new(&self.state.todo_items);
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

        // Tool display (only if active)
        if has_tools {
            let tool_display = ToolDisplayWidget::new(&self.state.active_tools);
            frame.render_widget(tool_display, chunks[3]);
        }

        // Task progress (only if active)
        if let Some(ref progress) = self.state.task_progress {
            let progress_widget =
                crate::widgets::progress::TaskProgressWidget::new(progress, &self.state.spinner);
            frame.render_widget(progress_widget, chunks[4]);
        }

        // Input
        let input = InputWidget::new(
            &self.state.input_buffer,
            self.state.input_cursor,
            self.state.agent_active,
            mode_str,
        );
        frame.render_widget(input, chunks[5]);

        // Autocomplete popup (rendered over conversation area)
        if self.state.autocomplete_visible && !self.state.autocomplete_suggestions.is_empty() {
            self.render_autocomplete(frame, chunks[5]);
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
        frame.render_widget(status, chunks[6]);
    }

    /// Render autocomplete popup above the input area.
    fn render_autocomplete(&self, frame: &mut ratatui::Frame, input_area: layout::Rect) {
        use crate::formatters::style_tokens;
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Paragraph};

        let suggestions = &self.state.autocomplete_suggestions;
        let max_show = suggestions.len().min(8);
        let popup_height = max_show as u16 + 2; // +2 for borders

        let popup_area = layout::Rect {
            x: input_area.x,
            y: input_area.y.saturating_sub(popup_height),
            width: input_area.width.min(50),
            height: popup_height,
        };

        let lines: Vec<Line> = suggestions
            .iter()
            .take(max_show)
            .enumerate()
            .map(|(i, cmd)| {
                let selected = i == self.state.autocomplete_index;
                let style = if selected {
                    Style::default()
                        .fg(style_tokens::CODE_BG)
                        .bg(style_tokens::CYAN)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(style_tokens::PRIMARY)
                };
                Line::from(vec![
                    Span::styled(format!("  /{:<16}", cmd.name), style),
                    Span::styled(
                        cmd.description.to_string(),
                        if selected {
                            Style::default()
                                .fg(style_tokens::CODE_BG)
                                .bg(style_tokens::CYAN)
                        } else {
                            Style::default().fg(style_tokens::SUBTLE)
                        },
                    ),
                ])
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(style_tokens::BORDER))
            .title(Span::styled(
                " Commands ",
                Style::default()
                    .fg(style_tokens::CYAN)
                    .add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(paragraph, popup_area);
    }

    /// Dispatch an event to the appropriate handler.
    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Resize(_, _) => {} // ratatui handles resize automatically
            AppEvent::Tick => self.handle_tick(),

            // Agent events
            AppEvent::AgentStarted => {
                self.state.agent_active = true;
            }
            AppEvent::AgentChunk(text) => {
                self.message_controller
                    .handle_agent_chunk(&mut self.state, &text);
            }
            AppEvent::AgentMessage(msg) => {
                self.message_controller
                    .handle_agent_message(&mut self.state, msg);
            }
            AppEvent::AgentFinished => {
                self.state.agent_active = false;
            }
            AppEvent::AgentError(err) => {
                self.state.agent_active = false;
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("Error: {err}"),
                    tool_call: None,
                });
            }

            // Thinking events
            AppEvent::ThinkingTrace(content) => {
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::Thinking,
                    content: format!("Thinking: {content}"),
                    tool_call: None,
                });
            }
            AppEvent::CritiqueTrace(content) => {
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::Thinking,
                    content: format!("Critique: {content}"),
                    tool_call: None,
                });
            }
            AppEvent::RefinedThinkingTrace(content) => {
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::Thinking,
                    content: format!("Refined: {content}"),
                    tool_call: None,
                });
            }

            // Tool events
            AppEvent::ToolStarted { tool_id, tool_name } => {
                self.state.active_tools.push(ToolExecution {
                    id: tool_id,
                    name: tool_name,
                    output_lines: Vec::new(),
                    finished: false,
                    success: false,
                    elapsed_secs: 0,
                    started_at: std::time::Instant::now(),
                    tick_count: 0,
                    parent_id: None,
                    depth: 0,
                });
            }
            AppEvent::ToolOutput { tool_id, output } => {
                if let Some(tool) = self.state.active_tools.iter_mut().find(|t| t.id == tool_id) {
                    tool.output_lines.push(output);
                }
            }
            AppEvent::ToolFinished { tool_id, success } => {
                if let Some(tool) = self.state.active_tools.iter_mut().find(|t| t.id == tool_id) {
                    tool.finished = true;
                    tool.success = success;
                }
                // Remove finished tools after a brief display period
                self.state.active_tools.retain(|t| !t.finished);
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
            }

            // Subagent events
            AppEvent::SubagentStarted {
                subagent_name,
                task,
            } => {
                self.state.active_subagents.push(
                    crate::widgets::nested_tool::SubagentDisplayState::new(subagent_name, task),
                );
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
            }
            AppEvent::TaskProgressFinished => {
                self.state.task_progress = None;
            }

            // UI events
            AppEvent::UserSubmit(ref msg) => {
                // Forward to backend if channel is configured
                if let Some(ref tx) = self.user_message_tx {
                    let _ = tx.send(msg.clone());
                    self.state.agent_active = true;
                }
            }
            AppEvent::Interrupt => {
                if self.state.agent_active {
                    self.interrupt_manager.interrupt();
                    self.state.agent_active = false;
                }
            }
            AppEvent::ModeChanged(mode) => {
                self.state.mode = match mode.as_str() {
                    "plan" => OperationMode::Plan,
                    _ => OperationMode::Normal,
                };
            }
            AppEvent::Quit => {
                self.state.running = false;
            }

            // Passthrough for unhandled terminal events
            AppEvent::Terminal(_) | AppEvent::Mouse(_) => {}
        }
    }

    /// Handle a key press event.
    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
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
            // Escape — interrupt agent
            (_, KeyCode::Esc) => {
                let _ = self.event_tx.send(AppEvent::Interrupt);
            }
            // Enter — submit message or execute slash command
            (_, KeyCode::Enter) => {
                if !self.state.input_buffer.is_empty() && !self.state.agent_active {
                    let msg = self.state.input_buffer.clone();
                    self.state.input_buffer.clear();
                    self.state.input_cursor = 0;
                    self.state.autocomplete_visible = false;
                    self.state.autocomplete_suggestions.clear();

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
                        let _ = self.event_tx.send(AppEvent::UserSubmit(msg));
                    }
                }
            }
            // Backspace
            (_, KeyCode::Backspace) => {
                if self.state.input_cursor > 0 {
                    self.state.input_cursor -= 1;
                    self.state.input_buffer.remove(self.state.input_cursor);
                }
            }
            // Delete
            (_, KeyCode::Delete) => {
                if self.state.input_cursor < self.state.input_buffer.len() {
                    self.state.input_buffer.remove(self.state.input_cursor);
                }
            }
            // Left arrow
            (_, KeyCode::Left) => {
                if self.state.input_cursor > 0 {
                    self.state.input_cursor -= 1;
                }
            }
            // Right arrow
            (_, KeyCode::Right) => {
                if self.state.input_cursor < self.state.input_buffer.len() {
                    self.state.input_cursor += 1;
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
            // Page Up — scroll conversation
            (_, KeyCode::PageUp) => {
                self.state.scroll_offset = self.state.scroll_offset.saturating_add(10);
                self.state.user_scrolled = true;
            }
            // Page Down — scroll conversation
            (_, KeyCode::PageDown) => {
                if self.state.scroll_offset > 0 {
                    self.state.scroll_offset = self.state.scroll_offset.saturating_sub(10);
                } else {
                    self.state.user_scrolled = false;
                }
            }
            // Shift+Tab — cycle mode
            (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                self.state.mode = match self.state.mode {
                    OperationMode::Normal => OperationMode::Plan,
                    OperationMode::Plan => OperationMode::Normal,
                };
            }
            // Ctrl+Shift+A — cycle autonomy level
            // crossterm delivers uppercase char when Shift is held
            (m, KeyCode::Char('A'))
                if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) =>
            {
                self.state.autonomy = match self.state.autonomy {
                    AutonomyLevel::Manual => AutonomyLevel::SemiAuto,
                    AutonomyLevel::SemiAuto => AutonomyLevel::Auto,
                    AutonomyLevel::Auto => AutonomyLevel::Manual,
                };
            }
            // Ctrl+Shift+T — cycle thinking level
            // crossterm delivers uppercase char when Shift is held
            (m, KeyCode::Char('T'))
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
                if self.state.autocomplete_visible
                    && !self.state.autocomplete_suggestions.is_empty()
                {
                    let cmd = self.state.autocomplete_suggestions[self.state.autocomplete_index];
                    self.state.input_buffer = format!("/{}", cmd.name);
                    self.state.input_cursor = self.state.input_buffer.len();
                    self.state.autocomplete_visible = false;
                    self.state.autocomplete_suggestions.clear();
                }
            }
            // Up/Down arrow — navigate autocomplete
            (_, KeyCode::Up) => {
                if self.state.autocomplete_visible
                    && !self.state.autocomplete_suggestions.is_empty()
                {
                    if self.state.autocomplete_index > 0 {
                        self.state.autocomplete_index -= 1;
                    }
                } else {
                    self.state.scroll_offset = self.state.scroll_offset.saturating_add(1);
                    self.state.user_scrolled = true;
                }
            }
            (_, KeyCode::Down) => {
                if self.state.autocomplete_visible
                    && !self.state.autocomplete_suggestions.is_empty()
                {
                    if self.state.autocomplete_index < self.state.autocomplete_suggestions.len() - 1
                    {
                        self.state.autocomplete_index += 1;
                    }
                } else if self.state.scroll_offset > 0 {
                    self.state.scroll_offset = self.state.scroll_offset.saturating_sub(1);
                } else {
                    self.state.user_scrolled = false;
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
                    });
                } else {
                    self.state.messages.push(DisplayMessage {
                        role: DisplayRole::System,
                        content: "No background tasks running.".to_string(),
                        tool_call: None,
                    });
                }
            }
            // Regular character input
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                self.state.input_buffer.insert(self.state.input_cursor, c);
                self.state.input_cursor += 1;
                // Update autocomplete on input change
                self.update_autocomplete();
            }
            _ => {}
        }
    }

    /// Update autocomplete suggestions based on current input.
    fn update_autocomplete(&mut self) {
        if self.state.input_buffer.starts_with('/') && !self.state.agent_active {
            let query = &self.state.input_buffer[1..]; // strip leading /
            self.state.autocomplete_suggestions =
                crate::controllers::slash_commands::find_matching_commands(query);
            self.state.autocomplete_visible = !self.state.autocomplete_suggestions.is_empty();
            self.state.autocomplete_index = 0;
        } else {
            self.state.autocomplete_visible = false;
            self.state.autocomplete_suggestions.clear();
        }
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
                });
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
                });
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
                });
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
                });
            }
            _ => {
                self.state.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!(
                        "Unknown command: /{name}. Type /help for available commands."
                    ),
                    tool_call: None,
                });
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

        // Update elapsed time and tick counter on active tools
        for tool in &mut self.state.active_tools {
            if !tool.finished {
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
        assert_eq!(app.state.scroll_offset, 10);
        assert!(app.state.user_scrolled);

        // Page down reduces offset but user is still scrolled
        let pgdn = crossterm::event::KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
        app.handle_key(pgdn);
        assert_eq!(app.state.scroll_offset, 0);
        // user_scrolled only clears when already at 0 and page down again
        assert!(app.state.user_scrolled);

        // One more page down at 0 clears user_scrolled
        app.handle_key(pgdn);
        assert!(!app.state.user_scrolled);
    }
}
