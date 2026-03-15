//! Ratatui-based terminal UI for the OpenDev AI coding assistant.
//!
//! This crate provides:
//! - [`app`] -- Main TUI application struct and event loop
//! - [`event`] -- Event types (keyboard, mouse, resize, agent messages)
//! - [`widgets`] -- UI widgets (conversation, input, status bar, tool display,
//!   spinner, progress, thinking, nested tool, todo panel)
//! - [`controllers`] -- Message handling, slash commands, approval prompts
//! - [`formatters`] -- Output formatting (markdown, display, tool colors)

pub mod app;
pub mod autocomplete;
pub mod controllers;
pub mod event;
pub mod formatters;
pub mod history;
pub mod managers;
pub mod widgets;

pub use app::{App, AppState, AutonomyLevel, OperationMode, ThinkingLevel};
pub use controllers::{
    ApprovalController, BUILTIN_COMMANDS, SlashCommand, find_matching_commands, is_command,
};
pub use event::{AppEvent, EventHandler};
pub use formatters::style_tokens::{
    TerminalBackground, Theme, ThemeName, auto_detect_theme, detect_terminal_background,
};
pub use formatters::{
    ToolCategory, categorize_tool, format_error, format_info, format_tool_call_display,
    format_tool_call_parts, format_warning, strip_system_reminders, tool_color, truncate_output,
};
pub use widgets::{
    SpinnerState, SubagentDisplayState, TaskProgress, ThinkingBlock, ThinkingPhase,
    TodoDisplayItem, TodoDisplayStatus, TodoPanelWidget,
};
