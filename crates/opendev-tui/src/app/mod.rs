//! Main TUI application struct and event loop.
//!
//! This module is split into focused sub-modules:
//! - [`enums`] — OperationMode, AutonomyLevel
//! - [`types`] — DisplayMessage, DisplayRole, RoleStyle, DisplayToolCall, ToolState, ToolExecution
//! - [`state`] — AppState struct and Default impl
//! - [`cache`] — Conversation message caching and incremental rebuild
//! - [`render`] — UI layout composition and main rendering orchestration
//! - [`render_popups`] — Popup panels and modal dialog rendering
//! - [`event_dispatch`] — Event routing and state mutations
//! - [`key_handler`] — Keyboard input handling
//! - [`slash_commands`] — Slash command execution
//! - [`tick`] — Tick-based animations and scroll acceleration

mod cache;
mod enums;
mod event_dispatch;
mod handle_agent;
mod handle_background;
mod handle_subagent;
mod handle_tools;
mod handle_ui;
mod key_handler;
mod render;
mod render_popups;
mod slash_commands;
mod state;
mod tick;
mod types;

pub use enums::{AutonomyLevel, OperationMode, ReasoningLevel};
pub use state::AppState;

/// Information collected for the exit message after TUI shutdown.
#[derive(Debug, Clone, Default)]
pub struct ExitInfo {
    pub session_id: Option<String>,
    pub session_cost: f64,
}
pub use types::{
    DisplayMessage, DisplayRole, DisplayToolCall, PendingItem, RoleStyle, ToolExecution, ToolState,
};

use std::io;
use std::sync::Arc;
use std::time::Duration;

use crate::controllers::{
    ApprovalController, AskUserController, McpCommandController, MessageController,
    ModelPickerController, PlanApprovalController,
};
use crate::event::{AppEvent, EventHandler};
use crate::managers::BackgroundTaskManager;
use crossterm::{
    event::{KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

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
    /// Ask-user controller for interactive question prompts.
    ask_user_controller: AskUserController,
    /// Oneshot sender to forward the ask-user answer back to the tool.
    ask_user_response_tx: Option<tokio::sync::oneshot::Sender<String>>,
    /// Approval controller for inline command approval prompts.
    approval_controller: ApprovalController,
    /// Oneshot sender to forward the approval decision back to the react loop.
    approval_response_tx:
        Option<tokio::sync::oneshot::Sender<opendev_runtime::ToolApprovalDecision>>,
    /// Plan approval controller for plan review prompts.
    plan_approval_controller: PlanApprovalController,
    /// Oneshot sender to forward the plan decision back to the tool.
    plan_approval_response_tx: Option<tokio::sync::oneshot::Sender<opendev_runtime::PlanDecision>>,
    /// Interrupt token for signaling cancellation to the agent (set per-query).
    interrupt_token: Option<opendev_runtime::InterruptToken>,
    /// Optional channel for forwarding user messages to the agent backend.
    user_message_tx: Option<mpsc::UnboundedSender<String>>,
    /// MCP command controller for managing MCP servers.
    mcp_controller: McpCommandController,
    /// Model picker controller for interactive model selection.
    model_picker_controller: Option<ModelPickerController>,
    /// Background task manager (shared with async kill tasks).
    task_manager: Arc<tokio::sync::Mutex<BackgroundTaskManager>>,
}

impl Default for App {
    fn default() -> Self {
        App::new()
    }
}

impl App {
    fn should_render_before_draining(event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::ReasoningContent(_)
                | AppEvent::AgentChunk(_)
                | AppEvent::AgentMessage(_)
                | AppEvent::ToolStarted { .. }
                | AppEvent::ToolResult { .. }
                | AppEvent::ToolFinished { .. }
                | AppEvent::SubagentStarted { .. }
                | AppEvent::SubagentToolCall { .. }
                | AppEvent::SubagentToolComplete { .. }
                | AppEvent::SubagentFinished { .. }
        )
    }

    /// Create a new TUI application with default state.
    pub fn new() -> Self {
        let event_handler = EventHandler::new(Duration::from_millis(60));
        let event_tx = event_handler.sender();
        Self {
            state: AppState::default(),
            event_handler,
            event_tx,
            message_controller: MessageController::new(),
            ask_user_controller: AskUserController::new(),
            ask_user_response_tx: None,
            approval_controller: ApprovalController::new(),
            approval_response_tx: None,
            plan_approval_controller: PlanApprovalController::new(),
            plan_approval_response_tx: None,
            interrupt_token: None,
            user_message_tx: None,
            mcp_controller: McpCommandController::new(vec![]),
            model_picker_controller: None,
            task_manager: Arc::new(tokio::sync::Mutex::new(BackgroundTaskManager::default())),
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
    pub async fn run(&mut self) -> io::Result<ExitInfo> {
        // Terminal setup
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;

        // Enable alternate scroll mode: terminal converts mouse wheel / trackpad
        // scroll into Up/Down arrow key sequences. Works reliably on macOS Terminal.app
        // where EnableMouseCapture doesn't produce scroll events for trackpad gestures.
        // Also enable focus change reporting for FocusGained/FocusLost redraws.
        {
            use std::io::Write;
            stdout.write_all(b"\x1b[?1007h")?;
            stdout.flush()?;
        }
        execute!(stdout, crossterm::event::EnableFocusChange)?;

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
        // Start the event reader
        self.event_handler.start();

        // Main loop
        let result = self.event_loop(&mut terminal).await;

        // Terminal teardown (always runs)
        if keyboard_enhanced {
            let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
        }
        disable_raw_mode()?;
        {
            use std::io::Write;
            let _ = terminal.backend_mut().write_all(b"\x1b[?1007l");
        }
        execute!(terminal.backend_mut(), crossterm::event::DisableFocusChange)?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result?;
        Ok(ExitInfo {
            session_id: self.state.session_id.clone(),
            session_cost: self.state.session_cost,
        })
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

            // Force a full screen repaint when needed (overlay close, focus regain, etc.).
            if self.state.force_clear {
                // Re-enable alternate scroll mode after focus regain (some terminals
                // reset it on focus change).
                {
                    use std::io::Write;
                    let _ = terminal.backend_mut().write_all(b"\x1b[?1007h");
                }
                // Full terminal clear: clear the backend screen AND reset both
                // internal ratatui diff buffers so the next draw() rewrites every cell.
                //
                // terminal.clear() sends ESC[2J (visual clear) and resets the back buffer.
                // swap_buffers() then resets the old current buffer (which may have stale
                // overlay content) and swaps, leaving both buffers empty.
                // This ensures the next draw() produces a complete diff with every cell
                // updated, eliminating stale overlay artifacts.
                let _ = terminal.clear();
                terminal.swap_buffers();
                self.state.force_clear = false;
                self.state.dirty = true;
            }

            // Only render when state has changed
            if self.state.dirty {
                // Rebuild cached conversation lines if messages changed, scroll
                // moved (scroll affects viewport culling boundaries), or terminal
                // width changed (cached lines are pre-wrapped to a specific width).
                let content_width = self.state.terminal_width.saturating_sub(1);
                if self.state.lines_generation != self.state.message_generation
                    || self.state.cached_scroll_offset != self.state.scroll_offset
                    || self.state.cached_width != content_width
                {
                    self.rebuild_cached_lines();
                    self.state.lines_generation = self.state.message_generation;
                    self.state.cached_scroll_offset = self.state.scroll_offset;
                }

                terminal.draw(|frame| self.render(frame))?;
                self.state.dirty = false;
                // Update selection geometry after render so mouse mapping uses fresh layout
                self.update_selection_geometry();
            }

            // Wait for at least one event
            let mut should_render_now = false;
            if let Some(event) = self.event_handler.next().await {
                should_render_now = Self::should_render_before_draining(&event);
                self.handle_event(event);
            }

            // Drain all remaining queued events before next render
            while !should_render_now {
                let Some(event) = self.event_handler.try_next() else {
                    break;
                };
                should_render_now = Self::should_render_before_draining(&event);
                self.handle_event(event);
                if !self.state.running {
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
