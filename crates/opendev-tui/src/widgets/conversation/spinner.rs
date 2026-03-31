//! Spinner and progress line rendering for active tools and subagents.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::formatters::style_tokens;
use crate::formatters::tool_line::{
    ToolLineStyle, format_elapsed, tool_line_active, tool_line_completed,
};
use crate::formatters::tool_registry::format_tool_call_parts_short;
use crate::widgets::spinner::{COMPACTION_CHAR, COMPLETED_CHAR, CONTINUATION_CHAR, SPINNER_FRAMES};

use crate::app::DisplayRole;

use super::ConversationWidget;

/// Format a token count for display: 500 → "500", 1234 → "1.2k", 1500000 → "1.5M".
fn format_token_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Return spinner color based on time since last token event.
///
/// - `None` or `< 3s` → blue (normal)
/// - `3–10s` → orange/warning (stalling)
/// - `> 10s` → red/error (likely stuck)
fn stall_color(stalled_secs: Option<u64>) -> ratatui::style::Color {
    match stalled_secs {
        Some(s) if s >= 10 => style_tokens::ERROR,
        Some(s) if s >= 3 => style_tokens::WARNING,
        _ => style_tokens::BLUE_BRIGHT,
    }
}

impl<'a> ConversationWidget<'a> {
    /// Get or create a `PathShortener` for this widget.
    fn get_shortener(&self) -> std::borrow::Cow<'_, crate::formatters::PathShortener> {
        if let Some(s) = self.shortener {
            std::borrow::Cow::Borrowed(s)
        } else {
            std::borrow::Cow::Owned(crate::formatters::PathShortener::new(Some(
                self.working_dir,
            )))
        }
    }

    /// Build spinner/progress lines appended to the conversation content.
    pub(crate) fn build_spinner_lines(&self) -> Vec<Line<'a>> {
        let mut lines: Vec<Line> = Vec::new();
        let shortener = self.get_shortener();

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
                        .fg(stall_color(self.stalled_secs))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Compacting conversation\u{2026}",
                    Style::default()
                        .fg(style_tokens::SUBTLE)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        } else if self.backgrounding_pending
            && !active_unfinished
                .iter()
                .any(|t| matches!(t.name.as_str(), "Agent" | "spawn_subagent"))
        {
            // Backgrounding feedback for non-subagent tools (e.g. bash, run_command).
            // When subagents are active, we fall through to the normal rendering loop
            // so the subagent list stays visible with per-agent "Sending to background…".
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", self.spinner_char),
                    Style::default().fg(stall_color(self.stalled_secs)),
                ),
                Span::styled(
                    "Sending to background\u{2026}",
                    Style::default()
                        .fg(style_tokens::SUBTLE)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        } else if !active_unfinished.is_empty() {
            for tool in &active_unfinished {
                let frame_idx = tool.tick_count % SPINNER_FRAMES.len();
                let spinner = SPINNER_FRAMES[frame_idx];

                if matches!(tool.name.as_str(), "Agent" | "spawn_subagent") {
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
                        (sa.name.clone(), sa.display_label().to_string())
                    } else {
                        let name = tool
                            .args
                            .get("agent_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Agent");
                        let desc = tool
                            .args
                            .get("description")
                            .and_then(|v| v.as_str())
                            .or_else(|| tool.args.get("task").and_then(|v| v.as_str()))
                            .unwrap_or("");
                        (name.to_string(), desc.to_string())
                    };

                    let task_desc = shortener.shorten_text(&task_desc);
                    let task_short = if task_desc.len() > 60 {
                        let end = task_desc.floor_char_boundary(60);
                        format!("{}...", &task_desc[..end])
                    } else {
                        task_desc
                    };

                    let mut spans = vec![
                        Span::styled(
                            format!("{spinner} "),
                            Style::default().fg(stall_color(self.stalled_secs)),
                        ),
                        Span::styled(
                            agent_name,
                            Style::default()
                                .fg(style_tokens::PRIMARY)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" {task_short}"),
                            Style::default().fg(style_tokens::SUBTLE),
                        ),
                    ];

                    // Ctrl+B hint: show after 2s of foreground subagent execution
                    if let Some(sa) = subagent
                        && sa.background_hint_shown
                        && !sa.backgrounded
                    {
                        spans.push(Span::styled(
                            "  Ctrl+B to background",
                            Style::default()
                                .fg(style_tokens::DIM_GREY)
                                .add_modifier(Modifier::ITALIC),
                        ));
                    }

                    lines.push(Line::from(spans));

                    if let Some(sa) = subagent {
                        self.build_subagent_spinner_lines(sa, &shortener, &mut lines);
                    }

                    lines.push(Line::from(""));
                } else {
                    // Normal tool: ⠋ verb arg Xs
                    let (verb, arg) =
                        format_tool_call_parts_short(&tool.name, &tool.args, &shortener);
                    lines.push(tool_line_active(
                        vec![],
                        spinner,
                        verb,
                        arg,
                        Some(format_elapsed(tool.elapsed_secs)),
                        ToolLineStyle::Primary,
                    ));
                }
            }
        } else if let Some(progress) = self.task_progress {
            // Skip TaskProgress spinner during active reasoning streaming —
            // the reasoning message renders its own "⟡ Thinking..." line
            let has_active_thinking =
                self.messages.iter().rev().any(|m| {
                    m.role == DisplayRole::Reasoning && m.thinking_duration_secs.is_none()
                });
            if !has_active_thinking {
                let elapsed = progress.started_at.elapsed().as_secs();
                let mut progress_spans = vec![
                    Span::styled(
                        format!("{} ", self.spinner_char),
                        Style::default().fg(stall_color(self.stalled_secs)),
                    ),
                    Span::styled(
                        if progress.description == "Thinking" {
                            if let Some(ctx) = self.last_tool_context {
                                format!("{ctx}... ")
                            } else {
                                format!("{}... ", self.thinking_verb)
                            }
                        } else {
                            format!("{}... ", progress.description)
                        },
                        if progress.description == "Thinking" {
                            // Fade from DIM_GREY to SUBTLE based on intensity
                            let (dr, dg, db) = (107u8, 114u8, 128u8); // DIM_GREY
                            let (sr, sg, sb) = (154u8, 160u8, 172u8); // SUBTLE
                            let t = self.verb_fade_intensity;
                            let r = dr as f32 + (sr as f32 - dr as f32) * t;
                            let g = dg as f32 + (sg as f32 - dg as f32) * t;
                            let b = db as f32 + (sb as f32 - db as f32) * t;
                            Style::default()
                                .fg(ratatui::style::Color::Rgb(r as u8, g as u8, b as u8))
                        } else {
                            Style::default().fg(style_tokens::SUBTLE)
                        },
                    ),
                    Span::styled(
                        format!("{}s", elapsed),
                        Style::default().fg(style_tokens::SUBTLE),
                    ),
                ];
                // Show token count after 30s of turn execution
                if self.turn_elapsed_secs >= 30 && self.turn_token_count > 0 {
                    progress_spans.push(Span::styled(
                        format!(
                            " \u{00b7} {} tokens",
                            format_token_count(self.turn_token_count)
                        ),
                        Style::default().fg(style_tokens::SUBTLE),
                    ));
                }
                progress_spans.push(Span::styled(
                    " (Esc to interrupt)",
                    Style::default().fg(style_tokens::SUBTLE),
                ));
                lines.push(Line::from(progress_spans));
            }
        } else if self
            .active_subagents
            .iter()
            .any(|s| !s.finished && !s.backgrounded)
        {
            // Leader is idle while subagents are still working
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", COMPACTION_CHAR),
                    Style::default().fg(style_tokens::ACCENT),
                ),
                Span::styled(
                    "Idle \u{00b7} teammates running",
                    Style::default()
                        .fg(style_tokens::SUBTLE)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        }

        lines
    }

    /// Build status lines for a subagent (unified for single and parallel).
    fn build_subagent_spinner_lines(
        &self,
        sa: &crate::widgets::nested_tool::SubagentDisplayState,
        shortener: &crate::formatters::PathShortener,
        lines: &mut Vec<Line<'a>>,
    ) {
        if self.backgrounding_pending {
            // During Ctrl+B transition, show a single "Sending to background…" sub-line
            // instead of the normal tool activity, so each subagent stays visible.
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {CONTINUATION_CHAR}  "),
                    Style::default().fg(style_tokens::GREY),
                ),
                Span::styled(
                    "Sending to background\u{2026}",
                    Style::default()
                        .fg(style_tokens::SUBTLE)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
            return;
        }

        if sa.finished {
            // Subagent finished but tool not yet — show Done summary
            let tool_count = sa.tool_call_count;
            let count_str = if tool_count > 0 {
                format!(" · {tool_count} tool uses")
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
            let (verb, arg) = format_tool_call_parts_short(&ct.tool_name, &ct.args, shortener);
            let continuation_prefix = vec![Span::styled(
                format!("  {CONTINUATION_CHAR}  "),
                Style::default().fg(style_tokens::GREY),
            )];
            lines.push(tool_line_completed(
                continuation_prefix,
                ct.success,
                verb,
                arg,
                None,
                ToolLineStyle::Nested,
            ));
        }

        // Show active tools with spinner
        for at in sa.active_tools.values() {
            let at_idx = at.tick % SPINNER_FRAMES.len();
            let at_ch = SPINNER_FRAMES[at_idx];
            let (verb, arg) = format_tool_call_parts_short(&at.tool_name, &at.args, shortener);
            let continuation_prefix = vec![Span::styled(
                format!("  {CONTINUATION_CHAR}  "),
                Style::default().fg(style_tokens::GREY),
            )];
            lines.push(tool_line_active(
                continuation_prefix,
                at_ch,
                verb,
                arg,
                None,
                ToolLineStyle::Nested,
            ));
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
        // Use tool_call_count (actual total) since completed_tools is capped at 100
        let total_completed = sa.tool_call_count.saturating_sub(sa.active_tools.len());
        let hidden = total_completed.saturating_sub(1);
        if hidden > 0 {
            lines.push(Line::from(Span::styled(
                format!("      +{hidden} more tool uses (Ctrl+B to run in background)"),
                Style::default()
                    .fg(style_tokens::GREY)
                    .add_modifier(Modifier::ITALIC),
            )));
        }
    }
}

#[cfg(test)]
#[path = "spinner_tests.rs"]
mod tests;
