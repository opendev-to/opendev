//! Conversation/chat display widget.
//!
//! Renders the conversation history with role-colored prefixes,
//! tool call summaries, thinking traces, system-reminder filtering,
//! collapsible tool results, and scroll support.

use std::borrow::Cow;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget, Wrap,
    },
};

use crate::app::{DisplayMessage, DisplayRole, DisplayToolCall, RoleStyle, ToolExecution};
use crate::formatters::display::strip_system_reminders;
use crate::formatters::markdown::MarkdownRenderer;
use crate::formatters::style_tokens::{self, Indent};
use crate::formatters::tool_registry::{categorize_tool, format_tool_call_parts_with_wd};
use crate::widgets::progress::TaskProgress;
use crate::widgets::spinner::{COMPACTION_CHAR, COMPLETED_CHAR, CONTINUATION_CHAR, SPINNER_FRAMES};

/// Check if a tool name is an edit/write tool that produces diffs.
pub fn is_diff_tool(name: &str) -> bool {
    matches!(name, "edit_file" | "write_file")
}

/// Type of a parsed diff entry.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffEntryType {
    Add,
    Del,
    Ctx,
}

/// A single parsed diff entry with line number and content.
#[derive(Debug, Clone)]
pub struct DiffEntry {
    pub entry_type: DiffEntryType,
    pub line_no: Option<usize>,
    pub content: String,
}

/// Reformat the summary line from the edit tool output.
///
/// Transforms e.g. `"Edited file.rs: 1 replacement(s), 2 addition(s) and 1 removal(s)"`
/// into `"Added 2 lines, removed 1 line"`.
fn reformat_summary(summary: &str) -> String {
    // Try to extract addition/removal counts from the summary
    let additions = extract_count(summary, "addition");
    let removals = extract_count(summary, "removal");

    if additions.is_none() && removals.is_none() {
        return summary.to_string();
    }

    let mut parts = Vec::new();
    if let Some(a) = additions.filter(|&a| a > 0) {
        let word = if a == 1 { "line" } else { "lines" };
        parts.push(format!("Added {a} {word}"));
    }
    if let Some(r) = removals.filter(|&r| r > 0) {
        let word = if r == 1 { "line" } else { "lines" };
        parts.push(format!("removed {r} {word}"));
    }
    if parts.is_empty() {
        return summary.to_string();
    }
    parts.join(", ")
}

/// Extract a count preceding a keyword like "addition" or "removal" from text.
fn extract_count(text: &str, keyword: &str) -> Option<usize> {
    let idx = text.find(keyword)?;
    let before = text[..idx].trim_end();
    before
        .rsplit_once(|c: char| !c.is_ascii_digit())
        .map(|(_, n)| n)
        .or(Some(before))
        .and_then(|n| n.parse().ok())
}

/// Parse unified diff text into structured entries with line numbers.
///
/// Returns (summary, entries) where summary is the first line (reformatted)
/// and entries are the parsed diff lines with line numbers.
pub fn parse_unified_diff(result_lines: &[String]) -> (String, Vec<DiffEntry>) {
    let mut entries = Vec::new();
    let mut summary = String::new();
    let mut old_line: usize = 0;
    let mut new_line: usize = 0;
    let mut seen_header = false;

    for (i, line) in result_lines.iter().enumerate() {
        if i == 0 {
            // First line is the summary
            summary = reformat_summary(line);
            continue;
        }

        // Skip file headers
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            continue;
        }

        // Parse hunk header
        if line.starts_with("@@") {
            seen_header = true;
            // Parse @@ -X,N +Y,M @@
            if let Some(rest) = line.strip_prefix("@@ -") {
                let parts: Vec<&str> = rest.splitn(2, '+').collect();
                if parts.len() == 2 {
                    // Parse old line number
                    if let Some(num_str) = parts[0].split(',').next() {
                        old_line = num_str.trim().parse().unwrap_or(1);
                    }
                    // Parse new line number
                    if let Some(num_part) = parts[1].split("@@").next()
                        && let Some(num_str) = num_part.split(',').next()
                    {
                        new_line = num_str.trim().parse().unwrap_or(1);
                    }
                }
            }
            continue;
        }

        if !seen_header {
            continue;
        }

        if let Some(content) = line.strip_prefix('+') {
            entries.push(DiffEntry {
                entry_type: DiffEntryType::Add,
                line_no: Some(new_line),
                content: content.to_string(),
            });
            new_line += 1;
        } else if let Some(content) = line.strip_prefix('-') {
            entries.push(DiffEntry {
                entry_type: DiffEntryType::Del,
                line_no: Some(old_line),
                content: content.to_string(),
            });
            old_line += 1;
        } else {
            // Context line — strip leading space if present
            let content = line.strip_prefix(' ').unwrap_or(line);
            entries.push(DiffEntry {
                entry_type: DiffEntryType::Ctx,
                line_no: Some(old_line),
                content: content.to_string(),
            });
            old_line += 1;
            new_line += 1;
        }
    }

    (summary, entries)
}

/// Render parsed diff entries as styled lines with right-aligned line numbers.
///
/// Add/del lines get a background color (green for additions, red for deletions).
/// The background is extended to fill the full row width by a post-render buffer
/// scan in `Widget::render()`.
pub fn render_diff_entries(entries: &[DiffEntry], lines: &mut Vec<Line<'_>>) {
    for entry in entries {
        let line_no_str = match entry.line_no {
            Some(n) => format!("{n:>4} "),
            None => "     ".to_string(),
        };
        let content = entry.content.replace('\t', "    ");

        let (operator, color, bg) = match entry.entry_type {
            DiffEntryType::Add => ("+ ", style_tokens::SUCCESS, Some(style_tokens::DIFF_ADD_BG)),
            DiffEntryType::Del => ("- ", style_tokens::ERROR, Some(style_tokens::DIFF_DEL_BG)),
            DiffEntryType::Ctx => ("  ", style_tokens::SUBTLE, None),
        };

        let content_str = format!("{operator}{content}");

        let line_no_style = match bg {
            Some(c) => Style::default().fg(style_tokens::SUBTLE).bg(c),
            None => Style::default().fg(style_tokens::SUBTLE),
        };
        let content_style = match bg {
            Some(c) => Style::default().fg(color).bg(c),
            None => Style::default().fg(color),
        };

        lines.push(Line::from(vec![
            Span::styled(line_no_str, line_no_style),
            Span::styled(content_str, content_style),
        ]));
    }
}

/// Widget that renders the conversation log.
pub struct ConversationWidget<'a> {
    messages: &'a [DisplayMessage],
    scroll_offset: u16,
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
}

impl<'a> ConversationWidget<'a> {
    pub fn new(messages: &'a [DisplayMessage], scroll_offset: u16) -> Self {
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
                DisplayRole::User | DisplayRole::System | DisplayRole::Interrupt => {
                    let rs = msg.role.style().unwrap();
                    Self::render_simple_role(&content, &rs, &mut lines);
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
                                // First line: ⟡ icon at same indent as ⏺ / > / !
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
                                // Continuation: 2-char indent matching other roles
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

            // Tool call summary with color coding
            if let Some(ref tc) = msg.tool_call {
                let tool_line = format_tool_call(tc, Some(self.working_dir));
                lines.push(tool_line);

                // Collapsible result lines (diff tools are never collapsed)
                let effective_collapsed = tc.collapsed && !is_diff_tool(&tc.name);
                if !effective_collapsed && !tc.result_lines.is_empty() {
                    let use_diff = is_diff_tool(&tc.name);
                    if use_diff {
                        let (summary, entries) = parse_unified_diff(&tc.result_lines);
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
                        render_diff_entries(&entries, &mut lines);
                    } else {
                        for (i, result_line) in tc.result_lines.iter().enumerate() {
                            let prefix_char: Cow<'static, str> = if i == 0 {
                                format!("  {}  ", CONTINUATION_CHAR).into()
                            } else {
                                Cow::Borrowed(Indent::RESULT_CONT)
                            };
                            lines.push(Line::from(vec![
                                Span::styled(
                                    prefix_char,
                                    Style::default().fg(style_tokens::SUBTLE),
                                ),
                                Span::styled(
                                    result_line.clone(),
                                    Style::default().fg(style_tokens::SUBTLE),
                                ),
                            ]));
                        }
                    }
                } else if effective_collapsed && !tc.result_lines.is_empty() {
                    // Show collapsed indicator — read tools get a short summary
                    let count = tc.result_lines.len();
                    let is_read = categorize_tool(&tc.name)
                        == crate::formatters::tool_registry::ToolCategory::FileRead;
                    let label = if is_read {
                        format!("  {}  ({count} lines)", CONTINUATION_CHAR)
                    } else {
                        format!(
                            "  {}  ({count} lines collapsed, press Ctrl+O to expand)",
                            CONTINUATION_CHAR,
                        )
                    };
                    lines.push(Line::from(Span::styled(
                        label,
                        Style::default().fg(style_tokens::SUBTLE),
                    )));
                }

                // Nested tool calls (from subagent execution)
                for nested in &tc.nested_calls {
                    let nested_line = format_nested_tool_call(nested, 1, Some(self.working_dir));
                    lines.push(nested_line);
                }
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

    /// Build spinner/progress lines separately from message content.
    ///
    /// These are rendered outside the scrollable area so that spinner
    /// animation (60ms ticks) doesn't shift scroll math or cause jitter.
    fn build_spinner_lines(&self) -> Vec<Line<'a>> {
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

                // For spawn_subagent, use Python-style nested display
                if tool.name == "spawn_subagent" {
                    // Find matching active subagent
                    let subagent = self.active_subagents.iter().find(|s| !s.finished);
                    let (agent_name, task_desc) = if let Some(sa) = subagent {
                        (sa.name.clone(), sa.task.clone())
                    } else {
                        // Fallback from args
                        let name = tool
                            .args
                            .get("agent_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Agent");
                        let task = tool
                            .args
                            .get("task")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        (name.to_string(), task.to_string())
                    };

                    // Truncate task
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

                    // Nested tool calls under ⎿
                    if let Some(sa) = subagent {
                        // Show last 3 completed tools
                        let start = sa.completed_tools.len().saturating_sub(3);
                        for ct in &sa.completed_tools[start..] {
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
                                Span::styled(
                                    format!("{icon} "),
                                    Style::default().fg(color),
                                ),
                                Span::styled(
                                    verb,
                                    Style::default().fg(style_tokens::SUBTLE),
                                ),
                                Span::styled(
                                    format!("({arg})"),
                                    Style::default().fg(style_tokens::GREY),
                                ),
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
                                Span::styled(
                                    verb,
                                    Style::default().fg(style_tokens::SUBTLE),
                                ),
                                Span::styled(
                                    format!("({arg})"),
                                    Style::default().fg(style_tokens::GREY),
                                ),
                            ]));
                        }

                        // If no tools yet, show initializing
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

                        // Hidden count
                        let hidden = sa.completed_tools.len().saturating_sub(3);
                        if hidden > 0 {
                            lines.push(Line::from(Span::styled(
                                format!("      +{hidden} more tool uses"),
                                Style::default()
                                    .fg(style_tokens::GREY)
                                    .add_modifier(Modifier::ITALIC),
                            )));
                        }
                    }
                } else {
                    // Normal tool: ⠋ verb(arg) (Xs)
                    let (verb, arg) = format_tool_call_parts_with_wd(&tool.name, &tool.args, Some(self.working_dir));
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
}

/// Format a tool call as a styled line with category color coding.
fn format_tool_call(tc: &DisplayToolCall, working_dir: Option<&str>) -> Line<'static> {
    let (icon, icon_color) = if tc.success {
        (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
    } else {
        (COMPLETED_CHAR, style_tokens::ERROR)
    };

    // For ask_user, display as "⏺ User answered Claude's questions:"
    if tc.name == "ask_user" {
        return Line::from(vec![
            Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
            Span::styled(
                "User answered Claude's questions:",
                Style::default()
                    .fg(style_tokens::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }

    let (verb, arg) = format_tool_call_parts_with_wd(&tc.name, &tc.arguments, working_dir);

    Line::from(vec![
        Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
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
    ])
}

/// Format a nested tool call with ⎿ continuation indent (Python style).
fn format_nested_tool_call(tc: &DisplayToolCall, _depth: usize, working_dir: Option<&str>) -> Line<'static> {
    let (icon, icon_color) = if tc.success {
        (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
    } else {
        ('\u{2717}', style_tokens::ERROR) // ✗
    };

    let (verb, arg) = format_tool_call_parts_with_wd(&tc.name, &tc.arguments, working_dir);

    Line::from(vec![
        Span::styled(
            format!("  {CONTINUATION_CHAR}  "),
            Style::default().fg(style_tokens::GREY),
        ),
        Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
        Span::styled(verb, Style::default().fg(style_tokens::SUBTLE)),
        Span::styled(
            format!("({arg})"),
            Style::default().fg(style_tokens::GREY),
        ),
    ])
}

impl Widget for ConversationWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 {
            return;
        }

        // Build spinner lines separately — these are rendered outside the
        // scrollable paragraph so that 60ms tick animation doesn't shift
        // the scroll math or cause the gap to jitter.
        let spinner_lines = self.build_spinner_lines();
        let spinner_height = spinner_lines.len() as u16;

        // Reserve bottom rows: 1 gap row + spinner rows (if any).
        // This keeps the gap between conversation and input stable.
        let reserved = 1 + spinner_height;
        let content_height = area.height.saturating_sub(reserved);
        if content_height == 0 {
            return;
        }

        let content_area = Rect {
            height: content_height,
            width: area.width.saturating_sub(1),
            ..area
        };

        // Use pre-built cached lines if available, otherwise build from scratch.
        let owned_lines;
        let lines: &[Line] = if let Some(cached) = self.cached_lines {
            cached
        } else {
            owned_lines = self.build_lines();
            &owned_lines
        };

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

        let paragraph = Paragraph::new(lines.to_vec()).wrap(Wrap { trim: false });

        // scroll_offset = lines from bottom; convert to lines from top for ratatui
        let clamped = (self.scroll_offset as usize).min(max_scroll);
        let actual_scroll = max_scroll.saturating_sub(clamped);

        paragraph
            .scroll((actual_scroll as u16, 0))
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

        // Render spinner lines below the scroll area.
        // Short conversation: position right after messages.
        // Long conversation: position at bottom of scroll area.
        if spinner_height > 0 {
            let spinner_y = if total_lines < viewport_height {
                // Short: right after the last message line
                content_area.y + total_lines as u16
            } else {
                // Long: fixed at bottom of scroll area
                content_area.y + content_area.height
            };

            for (i, line) in spinner_lines.iter().enumerate() {
                let y = spinner_y + i as u16;
                if y < area.bottom() {
                    buf.set_line(area.x, y, line, area.width);
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
mod tests {
    use super::*;
    use crate::app::DisplayMessage;

    #[test]
    fn test_empty_conversation() {
        let msgs: Vec<DisplayMessage> = vec![];
        let widget = ConversationWidget::new(&msgs, 0);
        let lines = widget.build_lines();
        // Welcome panel is now a separate widget; empty conversation returns no lines
        assert!(lines.is_empty());
    }

    #[test]
    fn test_user_message_rendering() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "Hello".into(),
            tool_call: None,
            collapsed: false,
        }];
        let widget = ConversationWidget::new(&msgs, 0);
        let lines = widget.build_lines();
        assert!(lines.len() >= 2); // message + blank line
    }

    #[test]
    fn test_tool_call_display() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::Assistant,
            content: "Running tool...".into(),
            tool_call: Some(DisplayToolCall {
                name: "bash".into(),
                arguments: std::collections::HashMap::new(),
                summary: Some("ls -la".into()),
                success: true,
                collapsed: false,
                result_lines: vec!["file1.rs".into(), "file2.rs".into()],
                nested_calls: vec![],
            }),
            collapsed: false,
        }];
        let widget = ConversationWidget::new(&msgs, 0);
        let lines = widget.build_lines();
        // message line + tool line + 2 result lines + blank
        assert!(lines.len() >= 5);
    }

    #[test]
    fn test_system_reminder_filtered() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::Assistant,
            content: "Hello<system-reminder>secret</system-reminder> world".into(),
            tool_call: None,
            collapsed: false,
        }];
        let widget = ConversationWidget::new(&msgs, 0);
        let lines = widget.build_lines();
        // Should not contain "secret"
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(!text.contains("secret"));
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
    }

    #[test]
    fn test_collapsed_tool_result() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::Assistant,
            content: "Done".into(),
            tool_call: Some(DisplayToolCall {
                name: "read_file".into(),
                arguments: std::collections::HashMap::new(),
                summary: Some("Read 100 lines".into()),
                success: true,
                collapsed: true,
                result_lines: vec!["line1".into(), "line2".into()],
                nested_calls: vec![],
            }),
            collapsed: false,
        }];
        let widget = ConversationWidget::new(&msgs, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // read_file shows short summary without "Ctrl+O" hint
        assert!(text.contains("2 lines"));
        assert!(!text.contains("Ctrl+O"));
    }

    #[test]
    fn test_spinner_active_tools() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "Do something".into(),
            tool_call: None,
            collapsed: false,
        }];
        let mut args = std::collections::HashMap::new();
        args.insert("command".into(), serde_json::Value::String("ls -la".into()));
        let tools = vec![ToolExecution {
            id: "t1".into(),
            name: "run_command".into(),
            output_lines: vec![],
            state: crate::app::ToolState::Running,
            elapsed_secs: 3,
            started_at: std::time::Instant::now(),
            tick_count: 0,
            parent_id: None,
            depth: 0,
            args,
        }];
        let widget = ConversationWidget::new(&msgs, 0).active_tools(&tools);
        // Spinner is now built separately from message lines
        let spinner = widget.build_spinner_lines();
        let text: String = spinner
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // Should show display name like "Bash(ls -la)" not raw "run_command"
        assert!(text.contains("Bash"));
        assert!(text.contains("ls -la"));
        assert!(text.contains("(3s)"));
        // Message lines should NOT contain spinner content
        let msg_lines = widget.build_lines();
        let msg_text: String = msg_lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(!msg_text.contains("(3s)"));
    }

    #[test]
    fn test_spinner_thinking() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "Hello".into(),
            tool_call: None,
            collapsed: false,
        }];
        let progress = TaskProgress {
            description: "Thinking".to_string(),
            elapsed_secs: 5,
            token_display: None,
            interrupted: false,
            started_at: std::time::Instant::now(),
        };
        let widget = ConversationWidget::new(&msgs, 0).task_progress(Some(&progress));
        let spinner = widget.build_spinner_lines();
        let text: String = spinner
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("Thinking..."));
        assert!(text.contains("esc to interrupt"));
    }

    #[test]
    fn test_spinner_tools_take_priority_over_thinking() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "Hello".into(),
            tool_call: None,
            collapsed: false,
        }];
        let tools = vec![ToolExecution {
            id: "t1".into(),
            name: "read_file".into(),
            output_lines: vec![],
            state: crate::app::ToolState::Running,
            elapsed_secs: 1,
            started_at: std::time::Instant::now(),
            tick_count: 2,
            parent_id: None,
            depth: 0,
            args: Default::default(),
        }];
        let progress = TaskProgress {
            description: "Thinking".to_string(),
            elapsed_secs: 5,
            token_display: None,
            interrupted: false,
            started_at: std::time::Instant::now(),
        };
        let widget = ConversationWidget::new(&msgs, 0)
            .active_tools(&tools)
            .task_progress(Some(&progress));
        let spinner = widget.build_spinner_lines();
        let text: String = spinner
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // Active tools shown with display name, not thinking
        assert!(text.contains("Read"));
        assert!(!text.contains("Thinking..."));
    }

    #[test]
    fn test_no_spinner_when_idle() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "Hello".into(),
            tool_call: None,
            collapsed: false,
        }];
        let widget = ConversationWidget::new(&msgs, 0);
        let spinner = widget.build_spinner_lines();
        assert!(spinner.is_empty());
        // Message lines: "> Hello" + blank separator
        let lines = widget.build_lines();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_nested_tool_calls() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::Assistant,
            content: "".into(),
            tool_call: Some(DisplayToolCall {
                name: "spawn_subagent".into(),
                arguments: std::collections::HashMap::new(),
                summary: Some("Exploring codebase".into()),
                success: true,
                collapsed: false,
                result_lines: vec![],
                nested_calls: vec![DisplayToolCall {
                    name: "read_file".into(),
                    arguments: std::collections::HashMap::new(),
                    summary: Some("src/main.rs".into()),
                    success: true,
                    collapsed: false,
                    result_lines: vec![],
                    nested_calls: vec![],
                }],
            }),
            collapsed: false,
        }];
        let widget = ConversationWidget::new(&msgs, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // spawn_subagent renders as AgentName(task), nested calls show formatted verb
        assert!(text.contains("Agent"));
        assert!(text.contains("Read"));
    }

    #[test]
    fn test_render_reserves_bottom_row_gap() {
        use ratatui::buffer::Buffer;

        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "Hello".into(),
            tool_call: None,
            collapsed: false,
        }];
        let widget = ConversationWidget::new(&msgs, 0);

        // Render into a small area
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // The last row (y=9) must be entirely blank — reserved gap
        for x in 0..40 {
            let cell = &buf[(x, 9)];
            assert_eq!(
                cell.symbol(),
                " ",
                "Bottom gap row should be blank at column {x}"
            );
        }
    }

    // ---------------------------------------------------------------
    // TUI snapshot tests using TestBackend
    // ---------------------------------------------------------------

    /// Extract visible text from a ratatui Buffer, row by row.
    fn buffer_text(buf: &ratatui::buffer::Buffer, area: Rect) -> Vec<String> {
        let mut rows = Vec::new();
        for y in area.y..area.bottom() {
            let mut row = String::new();
            for x in area.x..area.right() {
                row.push_str(buf[(x, y)].symbol());
            }
            rows.push(row.trim_end().to_string());
        }
        rows
    }

    #[test]
    fn test_snapshot_empty_conversation() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let msgs: Vec<DisplayMessage> = vec![];

        terminal
            .draw(|frame| {
                let widget = ConversationWidget::new(&msgs, 0);
                frame.render_widget(widget, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let rows = buffer_text(&buf, Rect::new(0, 0, 80, 24));
        // Empty conversation renders nothing — all rows should be blank
        for row in &rows {
            assert!(
                row.trim().is_empty(),
                "Expected blank row for empty conversation, got: {row:?}"
            );
        }
    }

    #[test]
    fn test_snapshot_single_user_message() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "What is Rust?".into(),
            tool_call: None,
            collapsed: false,
        }];

        terminal
            .draw(|frame| {
                let widget = ConversationWidget::new(&msgs, 0);
                frame.render_widget(widget, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let rows = buffer_text(&buf, Rect::new(0, 0, 80, 24));
        // First row should contain the user prompt marker and message
        assert!(
            rows[0].contains(">") && rows[0].contains("What is Rust?"),
            "First row should show user message, got: {:?}",
            rows[0]
        );
    }

    #[test]
    fn test_snapshot_multi_message_with_tool_call() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let msgs = vec![
            DisplayMessage {
                role: DisplayRole::User,
                content: "List files".into(),
                tool_call: None,
                collapsed: false,
            },
            DisplayMessage {
                role: DisplayRole::Assistant,
                content: "I'll list the files.".into(),
                tool_call: Some(DisplayToolCall {
                    name: "bash".into(),
                    arguments: std::collections::HashMap::new(),
                    summary: Some("ls -la".into()),
                    success: true,
                    collapsed: false,
                    result_lines: vec!["main.rs".into(), "lib.rs".into()],
                    nested_calls: vec![],
                }),
                collapsed: false,
            },
            DisplayMessage {
                role: DisplayRole::Assistant,
                content: "Here are the files.".into(),
                tool_call: None,
                collapsed: false,
            },
        ];

        terminal
            .draw(|frame| {
                let widget = ConversationWidget::new(&msgs, 0);
                frame.render_widget(widget, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let all_text: String = buffer_text(&buf, Rect::new(0, 0, 80, 24)).join("\n");

        // Verify key content appears in the rendered output
        assert!(all_text.contains("List files"), "Missing user message");
        assert!(
            all_text.contains("list the files"),
            "Missing assistant content"
        );
        assert!(
            all_text.contains("main.rs"),
            "Missing tool result line 'main.rs'"
        );
        assert!(
            all_text.contains("lib.rs"),
            "Missing tool result line 'lib.rs'"
        );
        assert!(
            all_text.contains("Here are the files"),
            "Missing second assistant message"
        );
    }

    #[test]
    fn test_snapshot_thinking_block() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let msgs = vec![
            DisplayMessage {
                role: DisplayRole::User,
                content: "Explain closures".into(),
                tool_call: None,
                collapsed: false,
            },
            DisplayMessage {
                role: DisplayRole::Thinking,
                content: "Let me think about closures in Rust...".into(),
                tool_call: None,
                collapsed: false,
            },
            DisplayMessage {
                role: DisplayRole::Assistant,
                content: "Closures capture variables from their scope.".into(),
                tool_call: None,
                collapsed: false,
            },
        ];

        terminal
            .draw(|frame| {
                let widget = ConversationWidget::new(&msgs, 0);
                frame.render_widget(widget, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let all_text: String = buffer_text(&buf, Rect::new(0, 0, 80, 24)).join("\n");

        assert!(
            all_text.contains("Explain closures"),
            "Missing user message"
        );
        assert!(
            all_text.contains("think about closures"),
            "Missing thinking content"
        );
        assert!(
            all_text.contains("capture variables"),
            "Missing assistant response"
        );
    }

    #[test]
    fn test_snapshot_scroll_indicator() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        // Create many messages to force scrolling in a small terminal
        let msgs: Vec<DisplayMessage> = (0..50)
            .map(|i| DisplayMessage {
                role: if i % 2 == 0 {
                    DisplayRole::User
                } else {
                    DisplayRole::Assistant
                },
                content: format!("Message number {i} with enough text to occupy a line"),
                tool_call: None,
                collapsed: false,
            })
            .collect();

        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        // Render with scroll offset > 0
        terminal
            .draw(|frame| {
                let widget = ConversationWidget::new(&msgs, 5);
                frame.render_widget(widget, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();

        // When scrolled, the rightmost column should contain scrollbar characters
        // (▲ at top, █ for thumb, ║ for track, ▼ at bottom)
        let last_col = 79u16;
        let right_col: String = (0..10u16)
            .map(|y| buf[(last_col, y)].symbol().to_string())
            .collect();
        let scrollbar_chars = ['▲', '█', '░', '▼', '║'];
        assert!(
            right_col.chars().any(|c| scrollbar_chars.contains(&c)),
            "Expected scrollbar characters in rightmost column when scrolled, got: {right_col:?}"
        );
    }

    #[test]
    fn test_collapsed_thinking_block() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::Thinking,
            content: "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6".into(),
            tool_call: None,
            collapsed: true,
        }];
        let widget = ConversationWidget::new(&msgs, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // Should show collapsed summary with Ctrl+O hint
        assert!(
            text.contains("Ctrl+O to expand"),
            "Collapsed thinking should show Ctrl+O hint, got: {text}"
        );
        assert!(
            text.contains("6 lines"),
            "Collapsed thinking should show line count, got: {text}"
        );
        // Should NOT show all lines
        assert!(
            !text.contains("Line 6"),
            "Collapsed thinking should not show all content"
        );
    }

    #[test]
    fn test_expanded_thinking_block() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::Thinking,
            content: "Line 1\nLine 2\nLine 3".into(),
            tool_call: None,
            collapsed: false,
        }];
        let widget = ConversationWidget::new(&msgs, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // Should show all lines
        assert!(
            text.contains("Line 1"),
            "Expanded thinking should show first line"
        );
        assert!(
            text.contains("Line 3"),
            "Expanded thinking should show last line"
        );
        // Should NOT show collapse hint
        assert!(
            !text.contains("Ctrl+O"),
            "Expanded thinking should not show Ctrl+O hint"
        );
    }

    #[test]
    fn test_parse_unified_diff_line_numbers() {
        let result_lines = vec![
            "Edited file.rs: 1 replacement(s), 2 addition(s) and 1 removal(s)".to_string(),
            "--- a/file.rs".to_string(),
            "+++ b/file.rs".to_string(),
            "@@ -201,5 +201,6 @@".to_string(),
            " context line".to_string(),
            "-old line".to_string(),
            "+new line 1".to_string(),
            "+new line 2".to_string(),
            " trailing context".to_string(),
        ];
        let (summary, entries) = parse_unified_diff(&result_lines);

        assert_eq!(summary, "Added 2 lines, removed 1 line");
        assert_eq!(entries.len(), 5);

        // Context line at 201
        assert_eq!(entries[0].entry_type, DiffEntryType::Ctx);
        assert_eq!(entries[0].line_no, Some(201));
        assert_eq!(entries[0].content, "context line");

        // Deletion at 202
        assert_eq!(entries[1].entry_type, DiffEntryType::Del);
        assert_eq!(entries[1].line_no, Some(202));
        assert_eq!(entries[1].content, "old line");

        // Additions at 202, 203
        assert_eq!(entries[2].entry_type, DiffEntryType::Add);
        assert_eq!(entries[2].line_no, Some(202));
        assert_eq!(entries[3].entry_type, DiffEntryType::Add);
        assert_eq!(entries[3].line_no, Some(203));

        // Trailing context at 203 (old), 204 (new)
        assert_eq!(entries[4].entry_type, DiffEntryType::Ctx);
        assert_eq!(entries[4].line_no, Some(203));
    }

    #[test]
    fn test_diff_rendering_with_line_numbers() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::Assistant,
            content: "".into(),
            tool_call: Some(DisplayToolCall {
                name: "edit_file".into(),
                arguments: std::collections::HashMap::new(),
                summary: None,
                success: true,
                collapsed: false,
                result_lines: vec![
                    "Edited file.rs: 1 replacement(s), 1 addition(s) and 1 removal(s)".into(),
                    "--- a/file.rs".into(),
                    "+++ b/file.rs".into(),
                    "@@ -10,3 +10,3 @@".into(),
                    " context".into(),
                    "-old".into(),
                    "+new".into(),
                ],
                nested_calls: vec![],
            }),
            collapsed: false,
        }];
        let widget = ConversationWidget::new(&msgs, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();

        // Should contain right-aligned line numbers
        assert!(text.contains("  10 "), "Should contain line number 10");
        assert!(text.contains("  11 "), "Should contain line number 11");
        // Should contain operators
        assert!(text.contains("+ new"), "Should contain '+ new'");
        assert!(text.contains("- old"), "Should contain '- old'");
        // Should contain reformatted summary
        assert!(
            text.contains("Added 1 line, removed 1 line"),
            "Should contain reformatted summary, got: {text}"
        );
        // Should NOT contain raw diff markers
        assert!(!text.contains("@@"), "Should not contain @@ hunk headers");
        assert!(!text.contains("--- a/"), "Should not contain file headers");
    }

    #[test]
    fn test_edit_tool_never_collapsed() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::Assistant,
            content: "".into(),
            tool_call: Some(DisplayToolCall {
                name: "edit_file".into(),
                arguments: std::collections::HashMap::new(),
                summary: None,
                success: true,
                collapsed: true, // Explicitly set collapsed
                result_lines: vec![
                    "Edited file.rs: 1 replacement(s), 1 addition(s) and 0 removal(s)".into(),
                    "--- a/file.rs".into(),
                    "+++ b/file.rs".into(),
                    "@@ -1,3 +1,4 @@".into(),
                    " line1".into(),
                    " line2".into(),
                    "+new line".into(),
                    " line3".into(),
                ],
                nested_calls: vec![],
            }),
            collapsed: false,
        }];
        let widget = ConversationWidget::new(&msgs, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();

        // Even though collapsed=true, edit_file should always show expanded
        assert!(
            !text.contains("collapsed"),
            "edit_file should never show collapsed indicator"
        );
        assert!(
            text.contains("+ new line"),
            "edit_file should show diff content even when collapsed=true"
        );
    }

    #[test]
    fn test_reformat_summary() {
        assert_eq!(
            reformat_summary("Edited file.rs: 1 replacement(s), 2 addition(s) and 1 removal(s)"),
            "Added 2 lines, removed 1 line"
        );
        assert_eq!(
            reformat_summary("Edited file.rs: 1 replacement(s), 0 addition(s) and 3 removal(s)"),
            "removed 3 lines"
        );
        assert_eq!(
            reformat_summary("Some unknown format"),
            "Some unknown format"
        );
    }
}
