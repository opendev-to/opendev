//! Nested tool display widget for subagent progress.
//!
//! Renders a tree-structured view of subagent tool calls,
//! showing which subagent is running, its active tool calls,
//! and completion status with tree connectors (similar to the
//! Python `NestedToolMixin`).

use std::collections::HashMap;
use std::time::Instant;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::formatters::style_tokens;
use crate::formatters::tool_registry::format_tool_call_parts_with_wd;

/// Tree connector characters (UTF-8 box drawing).
const TREE_BRANCH: &str = "\u{251c}\u{2500}";
const TREE_LAST: &str = "\u{2514}\u{2500}";
const TREE_VERTICAL: &str = "\u{2502}";

/// Spinner characters for cycling animation.
const SPINNER_CHARS: &[char] = &['\u{25cb}', '\u{25cf}', '\u{25cb}', '\u{25ce}'];

/// Green gradient colors for spinner animation.
const GREEN_GRADIENT: &[Color] = &[
    Color::Rgb(0, 180, 90),
    Color::Rgb(0, 200, 100),
    Color::Rgb(0, 220, 110),
    Color::Rgb(0, 240, 120),
];

/// State tracking for a single subagent execution.
#[derive(Debug, Clone)]
pub struct SubagentDisplayState {
    /// Unique identifier for this subagent instance.
    pub subagent_id: String,
    /// Subagent name.
    pub name: String,
    /// Task description.
    pub task: String,
    /// When the subagent started.
    pub started_at: Instant,
    /// Whether the subagent has finished.
    pub finished: bool,
    /// Whether the subagent succeeded (only valid when finished).
    pub success: bool,
    /// Final result summary (only valid when finished).
    pub result_summary: String,
    /// Total tool calls made.
    pub tool_call_count: usize,
    /// Active tool calls (tool_id -> NestedToolCallState).
    pub active_tools: HashMap<String, NestedToolCallState>,
    /// Completed tool calls (for display).
    pub completed_tools: Vec<CompletedToolCall>,
    /// Accumulated token count (input + output).
    pub token_count: u64,
    /// Animation tick counter for spinner.
    pub tick: usize,
    /// Optional shallow subagent warning.
    pub shallow_warning: Option<String>,
    /// When the subagent finished (for cleanup timing).
    pub finished_at: Option<Instant>,
}

impl SubagentDisplayState {
    /// Create a new subagent display state.
    pub fn new(subagent_id: String, name: String, task: String) -> Self {
        Self {
            subagent_id,
            name,
            task,
            started_at: Instant::now(),
            finished: false,
            success: false,
            result_summary: String::new(),
            tool_call_count: 0,
            active_tools: HashMap::new(),
            completed_tools: Vec::new(),
            token_count: 0,
            tick: 0,
            shallow_warning: None,
            finished_at: None,
        }
    }

    /// Record a new tool call starting.
    pub fn add_tool_call(
        &mut self,
        tool_name: String,
        tool_id: String,
        args: HashMap<String, serde_json::Value>,
    ) {
        self.tool_call_count += 1;
        self.active_tools.insert(
            tool_id.clone(),
            NestedToolCallState {
                tool_name,
                tool_id,
                args,
                started_at: Instant::now(),
                tick: 0,
            },
        );
    }

    /// Accumulate token usage from an LLM call.
    pub fn add_tokens(&mut self, input_tokens: u64, output_tokens: u64) {
        self.token_count += input_tokens + output_tokens;
    }

    /// Record a tool call completing.
    pub fn complete_tool_call(&mut self, tool_id: &str, success: bool) {
        if let Some(state) = self.active_tools.remove(tool_id) {
            self.completed_tools.push(CompletedToolCall {
                tool_name: state.tool_name,
                args: state.args,
                elapsed: state.started_at.elapsed(),
                success,
            });
            // Cap completed tools to prevent unbounded growth in long-running subagents
            if self.completed_tools.len() > 100 {
                self.completed_tools
                    .drain(..self.completed_tools.len() - 100);
            }
        }
    }

    /// Mark the subagent as finished.
    pub fn finish(
        &mut self,
        success: bool,
        result_summary: String,
        tool_call_count: usize,
        shallow_warning: Option<String>,
    ) {
        self.finished = true;
        self.finished_at = Some(Instant::now());
        self.success = success;
        self.result_summary = result_summary;
        self.tool_call_count = tool_call_count;
        self.shallow_warning = shallow_warning;
    }

    /// Advance the animation tick.
    pub fn advance_tick(&mut self) {
        self.tick += 1;
        for tool in self.active_tools.values_mut() {
            tool.tick += 1;
        }
    }

    /// Elapsed time since start.
    pub fn elapsed_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Generate a summary of current activity.
    pub fn activity_summary(&self) -> String {
        if self.active_tools.is_empty() {
            if self.finished {
                return "Done".to_string();
            }
            return "Running...".to_string();
        }

        // Count active tool types
        let mut read_count = 0usize;
        let mut search_count = 0usize;
        let mut other_name = None;
        for tool in self.active_tools.values() {
            match tool.tool_name.as_str() {
                "read_file" | "read_pdf" => read_count += 1,
                "search" | "list_files" | "find_symbol" | "find_referencing_symbols" => {
                    search_count += 1
                }
                name => other_name = Some(name.to_string()),
            }
        }

        if read_count > 1 {
            format!("Reading {} files...", read_count)
        } else if search_count > 1 {
            format!("Searching for {} patterns...", search_count)
        } else if let Some(name) = other_name {
            format!("{name}...")
        } else if read_count == 1 {
            "Reading...".to_string()
        } else {
            "Running...".to_string()
        }
    }
}

/// State for an active nested tool call.
#[derive(Debug, Clone)]
pub struct NestedToolCallState {
    pub tool_name: String,
    pub tool_id: String,
    pub args: HashMap<String, serde_json::Value>,
    pub started_at: Instant,
    pub tick: usize,
}

/// Record of a completed tool call.
#[derive(Debug, Clone)]
pub struct CompletedToolCall {
    pub tool_name: String,
    pub args: HashMap<String, serde_json::Value>,
    pub elapsed: std::time::Duration,
    pub success: bool,
}

/// Widget that renders the nested subagent tool display.
pub struct NestedToolWidget<'a> {
    subagents: &'a [SubagentDisplayState],
    working_dir: Option<&'a str>,
}

impl<'a> NestedToolWidget<'a> {
    pub fn new(subagents: &'a [SubagentDisplayState]) -> Self {
        Self {
            subagents,
            working_dir: None,
        }
    }

    pub fn working_dir(mut self, wd: &'a str) -> Self {
        self.working_dir = Some(wd);
        self
    }
}

impl Widget for NestedToolWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.subagents.is_empty() {
            return;
        }

        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(style_tokens::BORDER))
            .title(Span::styled(
                " Subagents ",
                Style::default()
                    .fg(style_tokens::HEADING_1)
                    .add_modifier(Modifier::BOLD),
            ));

        let mut lines: Vec<Line> = Vec::new();

        for (i, subagent) in self.subagents.iter().enumerate() {
            let is_last = i == self.subagents.len() - 1;

            // Subagent header line
            let connector = if is_last { TREE_LAST } else { TREE_BRANCH };
            let (status_icon, status_color) = if subagent.finished {
                if subagent.success {
                    ("\u{23fa}", style_tokens::SUCCESS)
                } else {
                    ("\u{23fa}", style_tokens::ERROR)
                }
            } else {
                let spinner_idx = subagent.tick % SPINNER_CHARS.len();
                let ch = SPINNER_CHARS[spinner_idx];
                let color_idx = subagent.tick % GREEN_GRADIENT.len();
                (&*format!("{ch}"), GREEN_GRADIENT[color_idx])
            };

            // Build a static string for the spinner character when not finished
            let spinner_string;
            let status_str = if subagent.finished {
                status_icon
            } else {
                let spinner_idx = subagent.tick % SPINNER_CHARS.len();
                spinner_string = SPINNER_CHARS[spinner_idx].to_string();
                &spinner_string
            };

            let elapsed = subagent.elapsed_secs();
            let task_preview = if subagent.task.len() > 60 {
                format!("{}...", &subagent.task[..60])
            } else {
                subagent.task.clone()
            };

            // Format elapsed as Xm Ys or Xs
            let elapsed_str = if elapsed >= 60 {
                format!("{}m {}s", elapsed / 60, elapsed % 60)
            } else {
                format!("{elapsed}s")
            };

            // Format token count
            let token_str = if subagent.token_count > 0 {
                let k = subagent.token_count as f64 / 1000.0;
                format!(" \u{00b7} {k:.1}k tokens")
            } else {
                String::new()
            };

            // Build stats suffix
            let stats = if subagent.finished {
                format!(
                    " ({} tool uses{} \u{00b7} {})",
                    subagent.tool_call_count, token_str, elapsed_str
                )
            } else {
                format!(
                    " ({} tool uses{} \u{00b7} {})",
                    subagent.tool_call_count, token_str, elapsed_str
                )
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {connector} "),
                    Style::default().fg(style_tokens::SUBTLE),
                ),
                Span::styled(format!("{status_str} "), Style::default().fg(status_color)),
                Span::styled(
                    subagent.name.clone(),
                    Style::default()
                        .fg(style_tokens::CYAN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(": {task_preview}"),
                    Style::default().fg(style_tokens::SUBTLE),
                ),
                Span::styled(stats, Style::default().fg(style_tokens::SUBTLE)),
            ]));

            // Show active tool calls
            let vertical = if is_last {
                "   "
            } else {
                &format!(" {TREE_VERTICAL}  ")
            };
            let active_count = subagent.active_tools.len();

            for (j, tool_state) in subagent.active_tools.values().enumerate() {
                let tool_is_last = j == active_count - 1 && subagent.completed_tools.is_empty();
                let tool_connector = if tool_is_last { TREE_LAST } else { TREE_BRANCH };
                let color_idx = tool_state.tick % GREEN_GRADIENT.len();
                let spinner_idx = tool_state.tick % SPINNER_CHARS.len();
                let spinner_ch = SPINNER_CHARS[spinner_idx];
                let tool_elapsed = tool_state.started_at.elapsed().as_secs();
                let (verb, arg) = format_tool_call_parts_with_wd(
                    &tool_state.tool_name,
                    &tool_state.args,
                    self.working_dir,
                );

                lines.push(Line::from(vec![
                    Span::styled(
                        format!(" {vertical}{tool_connector} "),
                        Style::default().fg(style_tokens::SUBTLE),
                    ),
                    Span::styled(
                        format!("{spinner_ch} "),
                        Style::default().fg(GREEN_GRADIENT[color_idx]),
                    ),
                    Span::styled(verb, Style::default().fg(style_tokens::SUBTLE)),
                    Span::styled(
                        format!("({arg})"),
                        Style::default().fg(style_tokens::SUBTLE),
                    ),
                    Span::styled(
                        format!(" ({tool_elapsed}s)"),
                        Style::default().fg(style_tokens::SUBTLE),
                    ),
                ]));
            }

            // Show last few completed tools (max 3)
            let completed_start = subagent.completed_tools.len().saturating_sub(3);
            let visible_completed = &subagent.completed_tools[completed_start..];
            for (j, completed) in visible_completed.iter().enumerate() {
                let is_last_tool = j == visible_completed.len() - 1;
                let tool_connector = if is_last_tool { TREE_LAST } else { TREE_BRANCH };
                let (icon, color) = if completed.success {
                    ("\u{23fa}", style_tokens::SUCCESS)
                } else {
                    ("\u{23fa}", style_tokens::ERROR)
                };
                let (verb, arg) = format_tool_call_parts_with_wd(
                    &completed.tool_name,
                    &completed.args,
                    self.working_dir,
                );

                lines.push(Line::from(vec![
                    Span::styled(
                        format!(" {vertical}{tool_connector} "),
                        Style::default().fg(style_tokens::SUBTLE),
                    ),
                    Span::styled(format!("{icon} "), Style::default().fg(color)),
                    Span::styled(verb, Style::default().fg(style_tokens::SUBTLE)),
                    Span::styled(
                        format!("({arg})"),
                        Style::default().fg(style_tokens::SUBTLE),
                    ),
                    Span::styled(
                        format!(" ({}s)", completed.elapsed.as_secs()),
                        Style::default().fg(style_tokens::SUBTLE),
                    ),
                ]));
            }

            // Show hidden count if there are more completed tools
            if completed_start > 0 {
                lines.push(Line::from(Span::styled(
                    format!(" {vertical}   +{completed_start} more tool uses"),
                    Style::default()
                        .fg(style_tokens::SUBTLE)
                        .add_modifier(Modifier::ITALIC),
                )));
            }

            // Show shallow warning if present
            if let Some(ref warning) = subagent.shallow_warning {
                lines.push(Line::from(Span::styled(
                    format!(" {vertical}   {warning}"),
                    Style::default().fg(style_tokens::WARNING),
                )));
            }
        }

        let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });

        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_display_state_new() {
        let state =
            SubagentDisplayState::new("id-1".into(), "Code-Explorer".into(), "Find TODOs".into());
        assert_eq!(state.name, "Code-Explorer");
        assert!(!state.finished);
        assert_eq!(state.tool_call_count, 0);
    }

    #[test]
    fn test_add_and_complete_tool_call() {
        let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
        state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
        assert_eq!(state.tool_call_count, 1);
        assert!(state.active_tools.contains_key("tc-1"));

        state.complete_tool_call("tc-1", true);
        assert!(state.active_tools.is_empty());
        assert_eq!(state.completed_tools.len(), 1);
        assert!(state.completed_tools[0].success);
    }

    #[test]
    fn test_finish() {
        let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
        state.finish(true, "Done".into(), 3, None);
        assert!(state.finished);
        assert!(state.success);
        assert_eq!(state.result_summary, "Done");
        assert_eq!(state.tool_call_count, 3);
    }

    #[test]
    fn test_finish_with_shallow_warning() {
        let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
        state.finish(
            true,
            "Done".into(),
            1,
            Some("Shallow subagent warning".into()),
        );
        assert!(state.shallow_warning.is_some());
    }

    #[test]
    fn test_advance_tick() {
        let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
        state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
        state.advance_tick();
        assert_eq!(state.tick, 1);
        assert_eq!(state.active_tools["tc-1"].tick, 1);
    }

    #[test]
    fn test_empty_widget() {
        let subagents: Vec<SubagentDisplayState> = vec![];
        let _widget = NestedToolWidget::new(&subagents);
    }

    #[test]
    fn test_widget_with_active_subagent() {
        let mut state =
            SubagentDisplayState::new("id-1".into(), "Code-Explorer".into(), "Find TODOs".into());
        state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
        let subagents = vec![state];
        let _widget = NestedToolWidget::new(&subagents);
    }

    #[test]
    fn test_token_accumulation() {
        let mut state = SubagentDisplayState::new("id-tok".into(), "test".into(), "task".into());
        assert_eq!(state.token_count, 0);
        state.add_tokens(1000, 500);
        assert_eq!(state.token_count, 1500);
        state.add_tokens(2000, 300);
        assert_eq!(state.token_count, 3800);
    }

    #[test]
    fn test_completed_tools_cap() {
        let mut state = SubagentDisplayState::new("id-cap".into(), "test".into(), "task".into());
        // Add 150 tool calls and complete them all
        for i in 0..150 {
            let id = format!("tc-{i}");
            state.add_tool_call("read_file".into(), id.clone(), HashMap::new());
            state.complete_tool_call(&id, true);
        }
        // Should be capped at 100
        assert_eq!(state.completed_tools.len(), 100);
        assert_eq!(state.tool_call_count, 150);
    }

    #[test]
    fn test_activity_summary_reading() {
        let mut state = SubagentDisplayState::new("id-act".into(), "test".into(), "task".into());
        state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
        state.add_tool_call("read_file".into(), "tc-2".into(), HashMap::new());
        state.add_tool_call("read_file".into(), "tc-3".into(), HashMap::new());
        assert_eq!(state.activity_summary(), "Reading 3 files...");
    }

    #[test]
    fn test_activity_summary_searching() {
        let mut state = SubagentDisplayState::new("id-act2".into(), "test".into(), "task".into());
        state.add_tool_call("search".into(), "tc-1".into(), HashMap::new());
        state.add_tool_call("list_files".into(), "tc-2".into(), HashMap::new());
        assert_eq!(state.activity_summary(), "Searching for 2 patterns...");
    }

    #[test]
    fn test_activity_summary_running() {
        let state = SubagentDisplayState::new("id-act3".into(), "test".into(), "task".into());
        assert_eq!(state.activity_summary(), "Running...");
    }

    #[test]
    fn test_activity_summary_done() {
        let mut state = SubagentDisplayState::new("id-act4".into(), "test".into(), "task".into());
        state.finish(true, "Done".into(), 0, None);
        assert_eq!(state.activity_summary(), "Done");
    }

    #[test]
    fn test_widget_with_finished_subagent() {
        let mut state =
            SubagentDisplayState::new("id-2".into(), "Planner".into(), "Create plan".into());
        state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
        state.complete_tool_call("tc-1", true);
        state.add_tool_call("write_file".into(), "tc-2".into(), HashMap::new());
        state.complete_tool_call("tc-2", true);
        state.finish(true, "Plan created".into(), 2, None);
        let subagents = vec![state];
        let _widget = NestedToolWidget::new(&subagents);
    }
}
