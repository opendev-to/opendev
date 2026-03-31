//! Display types for the TUI conversation view.

/// A message prepared for display in the conversation widget.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: DisplayRole,
    pub content: String,
    /// Optional tool call info for assistant messages.
    pub tool_call: Option<DisplayToolCall>,
    /// Whether this message is collapsed.
    pub collapsed: bool,
    /// When reasoning started (for computing finalized duration).
    pub thinking_started_at: Option<std::time::Instant>,
    /// Finalized thinking duration in seconds (set when thinking ends).
    pub thinking_duration_secs: Option<u64>,
    /// When thinking was finalized (for minimum 2s display to avoid flicker).
    pub thinking_finalized_at: Option<std::time::Instant>,
}

impl DisplayMessage {
    /// Convenience constructor for the common case (no tool call, not collapsed).
    pub fn new(role: DisplayRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call: None,
            collapsed: false,
            thinking_started_at: None,
            thinking_duration_secs: None,
            thinking_finalized_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayRole {
    User,
    Assistant,
    System,
    /// Interrupted feedback — rendered with ⎿ in red.
    Interrupt,
    /// Native reasoning content from the LLM (inline thinking).
    Reasoning,
    /// Slash command echo — rendered with `❯ ` prefix in accent+bold.
    SlashCommand,
    /// Slash command result — rendered with `  ⎿  ` prefix, attaches to previous.
    CommandResult,
    /// Plan content — rendered in a bordered panel with markdown.
    Plan,
}

/// Rendering configuration for simple (non-markdown, non-collapsible) roles.
pub struct RoleStyle {
    /// Prefix string for the first line (e.g. "> ", "! ", "  ⎿  ")
    pub icon: String,
    /// Style for the icon span
    pub icon_style: ratatui::style::Style,
    /// Color for the content text
    pub text_color: ratatui::style::Color,
    /// Continuation prefix for wrapped lines (must match icon visual width)
    pub continuation: &'static str,
    /// Whether to suppress the blank line before this message
    pub attach_to_previous: bool,
}

impl DisplayRole {
    /// Returns a `RoleStyle` for roles that use the standard icon+text pattern.
    /// Returns `None` for Assistant (it has custom rendering).
    pub fn style(&self) -> Option<RoleStyle> {
        use crate::formatters::style_tokens::{self, Indent};
        use crate::widgets::spinner::CONTINUATION_CHAR;
        use ratatui::style::{Modifier, Style};

        match self {
            Self::User => Some(RoleStyle {
                icon: "> ".to_string(),
                icon_style: Style::default()
                    .fg(style_tokens::ACCENT)
                    .add_modifier(Modifier::BOLD),
                text_color: style_tokens::PRIMARY,
                continuation: Indent::CONT,
                attach_to_previous: false,
            }),
            Self::Interrupt => Some(RoleStyle {
                icon: format!("  {CONTINUATION_CHAR}  "),
                icon_style: Style::default()
                    .fg(style_tokens::ERROR)
                    .add_modifier(Modifier::BOLD),
                text_color: style_tokens::ERROR,
                continuation: Indent::RESULT_CONT,
                attach_to_previous: true,
            }),
            Self::SlashCommand => Some(RoleStyle {
                icon: "❯ ".to_string(),
                icon_style: Style::default()
                    .fg(style_tokens::ACCENT)
                    .add_modifier(Modifier::BOLD),
                text_color: style_tokens::PRIMARY,
                continuation: Indent::CONT,
                attach_to_previous: false,
            }),
            Self::CommandResult => Some(RoleStyle {
                icon: format!("  {CONTINUATION_CHAR}  "),
                icon_style: Style::default().fg(style_tokens::ACCENT),
                text_color: style_tokens::SUBTLE,
                continuation: Indent::RESULT_CONT,
                attach_to_previous: true,
            }),
            Self::Assistant | Self::System | Self::Reasoning | Self::Plan => None,
        }
    }
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
    /// Error text for failed tools (shown in collapsed view).
    pub error_text: Option<String>,
}

impl DisplayToolCall {
    /// Convert a model `ToolCall` into a `DisplayToolCall` with smart collapse
    /// and result extraction.  Used by both history hydration and the batch
    /// message handler so they produce identical output.
    pub fn from_model(tc: &opendev_models::message::ToolCall) -> Self {
        use crate::formatters::tool_registry::{ToolCategory, categorize_tool};
        use crate::widgets::conversation::is_diff_tool;

        let result_lines: Vec<String> = tc
            .result
            .as_ref()
            .map(|r| {
                let text = match r {
                    serde_json::Value::String(s) => s.clone(),
                    other => serde_json::to_string_pretty(other).unwrap_or_default(),
                };
                text.lines().take(50).map(|l| l.to_string()).collect()
            })
            .unwrap_or_default();

        let category = categorize_tool(&tc.name);
        let is_file_read = category == ToolCategory::FileRead;
        let is_bash = category == ToolCategory::Bash;
        let collapsed = is_file_read
            || (is_bash && result_lines.len() > 4)
            || (!is_bash && result_lines.len() > 5 && !is_diff_tool(&tc.name));

        let nested_calls = tc
            .nested_tool_calls
            .iter()
            .map(DisplayToolCall::from_model)
            .collect();

        Self {
            name: tc.name.clone(),
            arguments: tc.parameters.clone(),
            summary: tc.result_summary.clone(),
            success: tc.error.is_none(),
            collapsed,
            result_lines,
            nested_calls,
            error_text: tc.error.clone(),
        }
    }
}

/// A queued item waiting to be processed by the foreground agent.
#[derive(Debug, Clone)]
pub enum PendingItem {
    /// A user message typed while the agent was busy.
    UserMessage(String),
    /// A completed background agent result.
    BackgroundResult {
        task_id: String,
        query: String,
        result: String,
        success: bool,
        tool_call_count: usize,
        cost_usd: f64,
    },
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

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
