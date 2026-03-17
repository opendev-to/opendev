//! Spinner and progress line rendering for active tools and subagents.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::formatters::style_tokens;
use crate::formatters::tool_registry::format_tool_call_parts_with_wd;
use crate::widgets::spinner::{COMPACTION_CHAR, COMPLETED_CHAR, CONTINUATION_CHAR, SPINNER_FRAMES};

use super::ConversationWidget;

impl<'a> ConversationWidget<'a> {
    /// Build spinner/progress lines separately from message content.
    ///
    /// These are rendered outside the scrollable area so that spinner
    /// animation (60ms ticks) doesn't shift scroll math or cause jitter.
    pub(super) fn build_spinner_lines(&self) -> Vec<Line<'a>> {
        let mut lines: Vec<Line> = Vec::new();

        let active_unfinished: Vec<_> = self
            .active_tools
            .iter()
            .filter(|t| !t.is_finished())
            .collect();

        if self.compaction_active {
            // Compaction spinner: ✻ Compacting conversation…
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", COMPACTION_CHAR),
                    Style::default()
                        .fg(style_tokens::BLUE_BRIGHT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Compacting conversation\u{2026}",
                    Style::default()
                        .fg(style_tokens::SUBTLE)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        } else if !active_unfinished.is_empty() {
            for tool in &active_unfinished {
                let frame_idx = tool.tick_count % SPINNER_FRAMES.len();
                let spinner = SPINNER_FRAMES[frame_idx];

                // For spawn_subagent, use nested display
                if tool.name == "spawn_subagent" {
                    // Match by parent_tool_id (reliable), fall back to task text.
                    let subagent = self
                        .active_subagents
                        .iter()
                        .find(|s| s.parent_tool_id.as_deref() == Some(&*tool.id))
                        .or_else(|| {
                            let tool_task =
                                tool.args.get("task").and_then(|v| v.as_str()).unwrap_or("");
                            self.active_subagents.iter().find(|s| s.task == tool_task)
                        });
                    let (agent_name, task_desc) = if let Some(sa) = subagent {
                        (sa.name.clone(), sa.task.clone())
                    } else {
                        let name = tool
                            .args
                            .get("agent_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Agent");
                        let task = tool.args.get("task").and_then(|v| v.as_str()).unwrap_or("");
                        (name.to_string(), task.to_string())
                    };

                    let task_short = if task_desc.len() > 60 {
                        format!("{}...", &task_desc[..60])
                    } else {
                        task_desc
                    };

                    // Header: ⠋ AgentName(task description)
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{spinner} "),
                            Style::default().fg(style_tokens::BLUE_BRIGHT),
                        ),
                        Span::styled(
                            agent_name,
                            Style::default()
                                .fg(style_tokens::PRIMARY)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("({task_short})"),
                            Style::default().fg(style_tokens::SUBTLE),
                        ),
                    ]));

                    if let Some(sa) = subagent {
                        self.build_subagent_spinner_lines(sa, &mut lines);
                    }

                    // Blank line between subagent blocks
                    lines.push(Line::from(""));
                } else {
                    // Normal tool: ⠋ verb(arg) (Xs)
                    let (verb, arg) = format_tool_call_parts_with_wd(
                        &tool.name,
                        &tool.args,
                        Some(self.working_dir),
                    );
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{spinner} "),
                            Style::default().fg(style_tokens::BLUE_BRIGHT),
                        ),
                        Span::styled(
                            verb,
                            Style::default()
                                .fg(style_tokens::PRIMARY)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("({arg})"),
                            Style::default().fg(style_tokens::SUBTLE),
                        ),
                        Span::styled(
                            format!(" ({}s)", tool.elapsed_secs),
                            Style::default().fg(style_tokens::GREY),
                        ),
                    ]));
                }
            }
        } else if let Some(progress) = self.task_progress {
            let elapsed = progress.started_at.elapsed().as_secs();
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", self.spinner_char),
                    Style::default().fg(style_tokens::BLUE_BRIGHT),
                ),
                Span::styled(
                    format!("{}... ", progress.description),
                    Style::default().fg(style_tokens::SUBTLE),
                ),
                Span::styled(
                    format!("({}s \u{00b7} esc to interrupt)", elapsed),
                    Style::default().fg(style_tokens::SUBTLE),
                ),
            ]));
        }

        lines
    }

    /// Build status lines for a subagent (unified for single and parallel).
    fn build_subagent_spinner_lines(
        &self,
        sa: &crate::widgets::nested_tool::SubagentDisplayState,
        lines: &mut Vec<Line<'a>>,
    ) {
        if sa.finished {
            // Subagent finished but tool not yet — show Done summary
            let tool_count = sa.tool_call_count;
            let count_str = if tool_count > 0 {
                format!(" ({tool_count} tool uses)")
            } else {
                String::new()
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {CONTINUATION_CHAR}  "),
                    Style::default().fg(style_tokens::GREY),
                ),
                Span::styled(
                    format!("{COMPLETED_CHAR} "),
                    Style::default().fg(style_tokens::GREEN_BRIGHT),
                ),
                Span::styled("Done", Style::default().fg(style_tokens::SUBTLE)),
                Span::styled(
                    count_str,
                    Style::default()
                        .fg(style_tokens::GREY)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
            return;
        }

        // Show last completed tool
        if let Some(ct) = sa.completed_tools.last() {
            let (icon, color) = if ct.success {
                (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
            } else {
                ('\u{2717}', style_tokens::ERROR)
            };
            let (verb, arg) =
                format_tool_call_parts_with_wd(&ct.tool_name, &ct.args, Some(self.working_dir));
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {CONTINUATION_CHAR}  "),
                    Style::default().fg(style_tokens::GREY),
                ),
                Span::styled(format!("{icon} "), Style::default().fg(color)),
                Span::styled(verb, Style::default().fg(style_tokens::SUBTLE)),
                Span::styled(format!("({arg})"), Style::default().fg(style_tokens::GREY)),
            ]));
        }

        // Show active tools with spinner
        for at in sa.active_tools.values() {
            let at_idx = at.tick % SPINNER_FRAMES.len();
            let at_ch = SPINNER_FRAMES[at_idx];
            let (verb, arg) =
                format_tool_call_parts_with_wd(&at.tool_name, &at.args, Some(self.working_dir));
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {CONTINUATION_CHAR}  "),
                    Style::default().fg(style_tokens::GREY),
                ),
                Span::styled(
                    format!("{at_ch} "),
                    Style::default().fg(style_tokens::BLUE_BRIGHT),
                ),
                Span::styled(verb, Style::default().fg(style_tokens::SUBTLE)),
                Span::styled(format!("({arg})"), Style::default().fg(style_tokens::GREY)),
            ]));
        }

        // Initializing if no tools yet
        if sa.active_tools.is_empty() && sa.completed_tools.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {CONTINUATION_CHAR}  "),
                    Style::default().fg(style_tokens::GREY),
                ),
                Span::styled(
                    "Initializing\u{2026}",
                    Style::default()
                        .fg(style_tokens::SUBTLE)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        }

        // "+N more tool uses" if hidden completed > 0
        let hidden = sa.completed_tools.len().saturating_sub(1);
        if hidden > 0 {
            lines.push(Line::from(Span::styled(
                format!("      +{hidden} more tool uses"),
                Style::default()
                    .fg(style_tokens::GREY)
                    .add_modifier(Modifier::ITALIC),
            )));
        }
    }
}
