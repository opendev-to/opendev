//! Conversation/chat display widget.
//!
//! Renders the conversation history with role-colored prefixes,
//! tool call summaries, thinking traces, system-reminder filtering,
//! collapsible tool results, and scroll support.
//!
//! This module is split into focused sub-modules:
//! - [`diff`] — Unified diff parsing and styled rendering
//! - [`spinner`] — Active tool spinner and progress line rendering
//! - [`tool_format`] — Tool call formatting helpers

mod diff;
mod spinner;
mod tool_format;

pub use diff::{DiffEntry, DiffEntryType, is_diff_tool, parse_unified_diff, render_diff_entries};

use std::borrow::Cow;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{
        Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget,
        Wrap,
    },
};

use crate::app::{DisplayMessage, DisplayRole, DisplayToolCall, RoleStyle, ToolExecution};
use crate::formatters::display::strip_system_reminders;
use crate::formatters::markdown::MarkdownRenderer;
use crate::formatters::style_tokens::{self, Indent};
use crate::formatters::tool_registry::ResultFormat;
use crate::widgets::progress::TaskProgress;
use crate::widgets::spinner::{COMPLETED_CHAR, CONTINUATION_CHAR, SPINNER_FRAMES};

use diff::{
    is_diff_tool as check_diff_tool, parse_unified_diff as parse_diff,
    render_diff_entries as render_diff,
};
use tool_format::{format_nested_tool_call, format_tool_call};

/// Widget that renders the conversation log.
pub struct ConversationWidget<'a> {
    messages: &'a [DisplayMessage],
    scroll_offset: u32,
    version: &'a str,
    working_dir: &'a str,
    mode: &'a str,
    /// Active tool executions (shown as inline spinners).
    active_tools: &'a [ToolExecution],
    /// Task progress (thinking state, shown when no active tools).
    task_progress: Option<&'a TaskProgress>,
    /// Pre-computed spinner character for the current frame.
    spinner_char: char,
    /// Whether manual compaction is in progress.
    compaction_active: bool,
    /// Pre-built cached lines for the static message portion (if available).
    cached_lines: Option<&'a [Line<'static>]>,
    /// Active subagent executions for nested inline display.
    active_subagents: &'a [crate::widgets::nested_tool::SubagentDisplayState],
    /// Cached path shortener (avoids repeated home_dir syscalls in spinner).
    shortener: Option<&'a crate::formatters::PathShortener>,
    /// Whether backgrounding is in progress (waiting for agent to yield).
    backgrounding_pending: bool,
    /// Current animated thinking verb (full text).
    thinking_verb: &'a str,
    /// Fade-in intensity for the thinking verb (0.0 = dim, 1.0 = bright).
    verb_fade_intensity: f32,
}

impl<'a> ConversationWidget<'a> {
    pub fn new(messages: &'a [DisplayMessage], scroll_offset: u32) -> Self {
        Self {
            messages,
            scroll_offset,
            version: "0.1.0",
            working_dir: ".",
            mode: "NORMAL",
            active_tools: &[],
            task_progress: None,
            spinner_char: SPINNER_FRAMES[0],
            compaction_active: false,
            cached_lines: None,
            active_subagents: &[],
            shortener: None,
            backgrounding_pending: false,
            thinking_verb: "Thinking",
            verb_fade_intensity: 1.0,
        }
    }

    pub fn version(mut self, version: &'a str) -> Self {
        self.version = version;
        self
    }

    pub fn working_dir(mut self, wd: &'a str) -> Self {
        self.working_dir = wd;
        self
    }

    pub fn mode(mut self, mode: &'a str) -> Self {
        self.mode = mode;
        self
    }

    pub fn active_tools(mut self, tools: &'a [ToolExecution]) -> Self {
        self.active_tools = tools;
        self
    }

    pub fn task_progress(mut self, progress: Option<&'a TaskProgress>) -> Self {
        self.task_progress = progress;
        self
    }

    pub fn spinner_char(mut self, ch: char) -> Self {
        self.spinner_char = ch;
        self
    }

    pub fn compaction_active(mut self, active: bool) -> Self {
        self.compaction_active = active;
        self
    }

    pub fn active_subagents(
        mut self,
        subagents: &'a [crate::widgets::nested_tool::SubagentDisplayState],
    ) -> Self {
        self.active_subagents = subagents;
        self
    }

    pub fn path_shortener(mut self, shortener: &'a crate::formatters::PathShortener) -> Self {
        self.shortener = Some(shortener);
        self
    }

    pub fn backgrounding_pending(mut self, pending: bool) -> Self {
        self.backgrounding_pending = pending;
        self
    }

    pub fn thinking_verb(mut self, verb: &'a str, fade_intensity: f32) -> Self {
        self.thinking_verb = verb;
        self.verb_fade_intensity = fade_intensity;
        self
    }

    /// Supply pre-built cached lines for the static message portion.
    /// When set, `build_lines()` is skipped and these lines are used directly,
    /// with dynamic spinner/progress lines still built fresh each frame.
    pub fn cached_lines(mut self, lines: &'a [Line<'static>]) -> Self {
        self.cached_lines = Some(lines);
        self
    }

    /// Render a message using the standard icon+text pattern.
    fn render_simple_role(content: &str, style: &RoleStyle, lines: &mut Vec<Line<'_>>) {
        for (i, line) in content.lines().enumerate() {
            if i == 0 {
                lines.push(Line::from(vec![
                    Span::styled(style.icon.clone(), style.icon_style),
                    Span::styled(line.to_string(), Style::default().fg(style.text_color)),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw(style.continuation),
                    Span::styled(line.to_string(), Style::default().fg(style.text_color)),
                ]));
            }
        }
    }

    /// Render plan content in a bordered panel with markdown formatting.
    fn render_plan_panel(content: &str, lines: &mut Vec<Line<'_>>) {
        let border_style = Style::default().fg(style_tokens::CYAN);
        let border_w: usize = 60;
        let inner_w = border_w.saturating_sub(1);
        let label = " Plan ";
        let top_after = border_w.saturating_sub(3 + label.len() + 1);

        // Top border: ╭── Plan ──────────────────╮
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}{}", style_tokens::BOX_TL, style_tokens::BOX_H.repeat(2)),
                border_style,
            ),
            Span::styled(
                label.to_string(),
                border_style.add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "{}{}",
                    style_tokens::BOX_H.repeat(top_after),
                    style_tokens::BOX_TR
                ),
                border_style,
            ),
        ]));

        // Top padding
        lines.push(Line::from(vec![
            Span::styled(style_tokens::BOX_V.to_string(), border_style),
            Span::raw(" ".repeat(inner_w.saturating_sub(1))),
            Span::styled(style_tokens::BOX_V.to_string(), border_style),
        ]));

        // Render content through markdown with left border prefix
        let md_lines = MarkdownRenderer::render(content);
        let prefix = format!("{}  ", style_tokens::BOX_V);
        for md_line in md_lines {
            let mut spans = vec![Span::styled(prefix.clone(), border_style)];
            spans.extend(md_line.spans);
            let line = Line::from(spans);
            let line_w = line.width();
            let mut spans = line.spans;
            let pad = border_w.saturating_sub(line_w);
            if pad > 0 {
                spans.push(Span::raw(" ".repeat(pad)));
            }
            spans.push(Span::styled(style_tokens::BOX_V.to_string(), border_style));
            lines.push(Line::from(spans));
        }

        // Bottom padding
        lines.push(Line::from(vec![
            Span::styled(style_tokens::BOX_V.to_string(), border_style),
            Span::raw(" ".repeat(inner_w.saturating_sub(1))),
            Span::styled(style_tokens::BOX_V.to_string(), border_style),
        ]));

        // Bottom border: ╰──────────────────────────╯
        lines.push(Line::from(vec![Span::styled(
            format!(
                "{}{}{}",
                style_tokens::BOX_BL,
                style_tokens::BOX_H.repeat(border_w.saturating_sub(2)),
                style_tokens::BOX_BR
            ),
            border_style,
        )]));
    }

    /// Build styled lines from messages.
    fn build_lines(&self) -> Vec<Line<'a>> {
        let mut lines: Vec<Line> = Vec::new();

        if self.messages.is_empty() {
            // Welcome panel is now rendered as a separate widget (WelcomePanelWidget)
            return lines;
        }

        for (msg_idx, msg) in self.messages.iter().enumerate() {
            // Filter system reminders from displayed content
            let content = strip_system_reminders(&msg.content);

            // Skip empty messages after filtering
            if content.is_empty() && msg.tool_call.is_none() {
                continue;
            }

            match msg.role {
                DisplayRole::Assistant => {
                    // Use markdown renderer for assistant messages
                    let md_lines = MarkdownRenderer::render(&content);
                    let mut leading_consumed = false;
                    for md_line in md_lines.into_iter() {
                        // Check if this line has non-empty content
                        let line_text: String = md_line
                            .spans
                            .iter()
                            .map(|s| s.content.to_string())
                            .collect();
                        let has_content = !line_text.trim().is_empty();

                        if !leading_consumed && has_content {
                            // First non-empty line gets ⏺ leading marker (green)
                            let mut spans = vec![Span::styled(
                                format!("{} ", COMPLETED_CHAR),
                                Style::default().fg(style_tokens::GREEN_BRIGHT),
                            )];
                            spans.extend(md_line.spans);
                            lines.push(Line::from(spans));
                            leading_consumed = true;
                        } else {
                            let mut spans = vec![Span::raw(Indent::CONT)];
                            spans.extend(md_line.spans);
                            lines.push(Line::from(spans));
                        }
                    }
                }
                DisplayRole::System => {
                    let subtle_style = Style::default().fg(style_tokens::SUBTLE);
                    for (i, line_text) in content.lines().enumerate() {
                        if i == 0 {
                            lines.push(Line::from(vec![
                                Span::styled(
                                    format!("{} ", COMPLETED_CHAR),
                                    Style::default().fg(style_tokens::WARNING),
                                ),
                                Span::styled(line_text.to_string(), subtle_style),
                            ]));
                        } else {
                            lines.push(Line::from(vec![
                                Span::raw(Indent::CONT),
                                Span::styled(line_text.to_string(), subtle_style),
                            ]));
                        }
                    }
                }
                DisplayRole::User
                | DisplayRole::Interrupt
                | DisplayRole::SlashCommand
                | DisplayRole::CommandResult => {
                    let rs = msg.role.style().unwrap();
                    Self::render_simple_role(&content, &rs, &mut lines);
                }
                DisplayRole::Reasoning => {
                    let thinking_style = Style::default().fg(style_tokens::THINKING_BG);

                    if msg.collapsed {
                        if let Some(secs) = msg.thinking_duration_secs {
                            // Finalized collapsed: single summary line
                            let duration_text = if secs == 0 {
                                "<1".to_string()
                            } else {
                                secs.to_string()
                            };
                            lines.push(Line::from(vec![
                                Span::styled(
                                    format!(
                                        "{} Thought for {}s",
                                        style_tokens::THINKING_ICON,
                                        duration_text
                                    ),
                                    thinking_style,
                                ),
                                Span::styled(
                                    " (Ctrl+I to expand)",
                                    Style::default().fg(style_tokens::SUBTLE),
                                ),
                            ]));
                        } else if let Some(started) = msg.thinking_started_at {
                            // Streaming: shimmer wave animation
                            let elapsed = started.elapsed().as_secs();
                            let text =
                                format!("{} Thinking... {}s", style_tokens::THINKING_ICON, elapsed);
                            let highlight = ratatui::style::Color::Rgb(200, 200, 220);
                            let mut spans = style_tokens::shimmer_line(
                                &text,
                                0, // fallback path has no tick_count
                                style_tokens::THINKING_BG,
                                highlight,
                            );
                            spans.push(Span::styled(
                                " (Ctrl+I to expand)",
                                Style::default().fg(style_tokens::SUBTLE),
                            ));
                            lines.push(Line::from(spans));
                        }
                    } else {
                        // Expanded: full markdown rendering
                        let md_lines =
                            MarkdownRenderer::render_muted(&content, style_tokens::THINKING_BG);
                        let mut leading_consumed = false;
                        for md_line in md_lines.into_iter() {
                            let line_text: String = md_line
                                .spans
                                .iter()
                                .map(|s| s.content.to_string())
                                .collect();
                            let has_content = !line_text.trim().is_empty();

                            if !leading_consumed && has_content {
                                let mut spans = vec![Span::styled(
                                    format!("{} ", style_tokens::THINKING_ICON),
                                    thinking_style,
                                )];
                                spans.extend(md_line.spans);
                                lines.push(Line::from(spans));
                                leading_consumed = true;
                            } else {
                                let mut spans =
                                    vec![Span::styled(Indent::THINKING_CONT, thinking_style)];
                                spans.extend(md_line.spans);
                                lines.push(Line::from(spans));
                            }
                        }
                    }
                }
                DisplayRole::Plan => {
                    Self::render_plan_panel(&content, &mut lines);
                }
            }

            // Tool call summary with color coding
            if let Some(ref tc) = msg.tool_call {
                self.build_tool_call_lines(tc, &mut lines);
            }

            // Blank line between messages — skip before messages that attach to previous
            let next_attaches = self
                .messages
                .get(msg_idx + 1)
                .and_then(|m| m.role.style())
                .is_some_and(|s| s.attach_to_previous);
            if !next_attaches {
                lines.push(Line::from(""));
            }
        }

        lines
    }

    fn build_render_lines(&self) -> Vec<Line<'a>> {
        let mut lines: Vec<Line<'a>> = if let Some(cached) = self.cached_lines {
            cached.to_vec()
        } else {
            self.build_lines()
        };

        let spinner_lines = self.build_spinner_lines();
        if !spinner_lines.is_empty() {
            lines.extend(spinner_lines);
        }

        lines
    }

    /// Build lines for a tool call result.
    fn build_tool_call_lines(&self, tc: &DisplayToolCall, lines: &mut Vec<Line<'a>>) {
        let tool_line = format_tool_call(tc, Some(self.working_dir));
        lines.push(tool_line);

        let is_bash = crate::formatters::tool_registry::lookup_tool(&tc.name).result_format
            == ResultFormat::Bash;

        // Collapsible result lines (diff tools are never collapsed)
        let effective_collapsed = tc.collapsed && !check_diff_tool(&tc.name);
        if !effective_collapsed && !tc.result_lines.is_empty() {
            let use_diff = check_diff_tool(&tc.name);
            if use_diff {
                let (summary, entries) = parse_diff(&tc.result_lines);
                // Summary line with ⎿ prefix
                if !summary.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {}  ", CONTINUATION_CHAR),
                            Style::default().fg(style_tokens::GREY),
                        ),
                        Span::styled(summary, Style::default().fg(style_tokens::SUBTLE)),
                    ]));
                }
                render_diff(entries.as_slice(), lines);
            } else {
                for (i, result_line) in tc.result_lines.iter().enumerate() {
                    let prefix_char: Cow<'static, str> = if i == 0 {
                        format!("  {}  ", CONTINUATION_CHAR).into()
                    } else {
                        Cow::Borrowed(Indent::RESULT_CONT)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(prefix_char, Style::default().fg(style_tokens::SUBTLE)),
                        Span::styled(
                            strip_ansi(result_line),
                            Style::default().fg(style_tokens::SUBTLE),
                        ),
                    ]));
                }
            }
        } else if effective_collapsed {
            if is_bash {
                lines.extend(build_bash_preview(&tc.result_lines));
            } else if !tc.result_lines.is_empty() {
                let count = tc.result_lines.len();
                let verb = crate::formatters::tool_registry::lookup_tool(&tc.name).verb;
                let label = if !tc.success {
                    format!(
                        "  {}  {verb} {count} lines (Ctrl+O to expand)",
                        CONTINUATION_CHAR
                    )
                } else {
                    format!("  {}  {verb} {count} lines", CONTINUATION_CHAR)
                };
                lines.push(Line::from(Span::styled(
                    label,
                    Style::default().fg(style_tokens::SUBTLE),
                )));
            }
        } else if tc.result_lines.is_empty() && is_bash {
            // Empty bash output: show "(no output)"
            lines.extend(build_bash_preview(&tc.result_lines));
        }

        // Nested tool calls (from subagent execution)
        for nested in &tc.nested_calls {
            let nested_line = format_nested_tool_call(nested, 1, Some(self.working_dir));
            lines.push(nested_line);
        }
    }
}

/// Strip ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
    use std::sync::LazyLock;
    static RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\x1B\[[\d;]*[A-Za-z]|\x1B[@-_][0-?]*[ -/]*[@-~]").unwrap()
    });
    RE.replace_all(s, "").to_string()
}

/// Build a Codex-style inline preview for Bash tool results.
///
/// - Empty output → single `(no output)` line
/// - ≤4 lines → all lines shown inline
/// - >4 lines → first 2, `… +N lines`, last 2
pub(crate) fn build_bash_preview(result_lines: &[String]) -> Vec<Line<'static>> {
    let text_color = style_tokens::SUBTLE;
    let total = result_lines.len();

    // Helper: build a result line with the appropriate prefix
    let make_line = |idx: usize, text: &str| {
        let prefix: Cow<'static, str> = if idx == 0 {
            format!("  {}  ", CONTINUATION_CHAR).into()
        } else {
            Cow::Borrowed(Indent::RESULT_CONT)
        };
        Line::from(vec![
            Span::styled(prefix, Style::default().fg(style_tokens::GREY)),
            Span::styled(strip_ansi(text), Style::default().fg(text_color)),
        ])
    };

    if total == 0 {
        return vec![make_line(0, "(no output)")];
    }

    let mut lines = Vec::new();
    if total <= 4 {
        for (i, line) in result_lines.iter().enumerate() {
            lines.push(make_line(i, line));
        }
    } else {
        // First 2 lines
        lines.push(make_line(0, &result_lines[0]));
        lines.push(make_line(1, &result_lines[1]));
        // Ellipsis with hidden line count
        let hidden = total - 4;
        lines.push(make_line(2, &format!("… +{hidden} lines")));
        // Last 2 lines
        lines.push(make_line(3, &result_lines[total - 2]));
        lines.push(make_line(4, &result_lines[total - 1]));
    }
    lines
}

impl Widget for ConversationWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 {
            return;
        }

        // Clear entire area to prevent stale cell artifacts during scrolling.
        // Ratatui's diff-based rendering can leave ghost content when scroll
        // shifts text and the same characters appear at different positions.
        Clear.render(area, buf);

        // Reserve a single blank row above input; spinner lines are part of the
        // scrollable conversation content.
        let reserved = 1;
        let content_height = area.height.saturating_sub(reserved);
        if content_height == 0 {
            return;
        }

        let content_area = Rect {
            height: content_height,
            width: area.width.saturating_sub(1),
            ..area
        };

        let render_lines = self.build_render_lines();
        let lines: &[Line] = &render_lines;

        // Compute total wrapped line count (character-level estimate)
        let total_lines: usize = lines
            .iter()
            .map(|line| {
                let w = line.width();
                if w == 0 || content_area.width == 0 {
                    1
                } else {
                    w.div_ceil(content_area.width as usize)
                }
            })
            .sum();
        let viewport_height = content_area.height as usize;
        let max_scroll = total_lines.saturating_sub(viewport_height);

        let paragraph = Paragraph::new(render_lines).wrap(Wrap { trim: false });

        // scroll_offset = lines from bottom; convert to lines from top for ratatui
        let clamped = (self.scroll_offset as usize).min(max_scroll);
        let actual_scroll = max_scroll.saturating_sub(clamped);

        paragraph
            .scroll((actual_scroll.min(u16::MAX as usize) as u16, 0))
            .render(content_area, buf);

        // Extend diff background colors to fill entire row width.
        // After rendering, scan each row — if any cell has a diff bg color,
        // fill all cells in that row with that background. This is resize-safe
        // since it operates on the actual rendered buffer dimensions.
        for y in content_area.y..content_area.y.saturating_add(content_area.height) {
            let mut diff_bg = None;
            for x in content_area.x..content_area.x.saturating_add(content_area.width) {
                if let Some(cell) = buf.cell(ratatui::layout::Position::new(x, y))
                    && (cell.bg == style_tokens::DIFF_ADD_BG
                        || cell.bg == style_tokens::DIFF_DEL_BG)
                {
                    diff_bg = Some(cell.bg);
                    break;
                }
            }
            if let Some(bg) = diff_bg {
                for x in content_area.x..content_area.x.saturating_add(content_area.width) {
                    if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(x, y)) {
                        cell.set_bg(bg);
                    }
                }
            }
        }

        // Visual scrollbar when content overflows
        if max_scroll > 0 {
            let mut scrollbar_state = ScrollbarState::new(max_scroll)
                .position(actual_scroll)
                .viewport_content_length(viewport_height);
            StatefulWidget::render(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area,
                buf,
                &mut scrollbar_state,
            );
        }
    }
}

#[cfg(test)]
mod tests;
