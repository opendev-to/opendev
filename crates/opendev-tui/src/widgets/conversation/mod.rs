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
    /// Cached path shortener (avoids repeated home_dir syscalls in spinner).
    shortener: Option<&'a crate::formatters::PathShortener>,
    /// Whether backgrounding is in progress (waiting for agent to yield).
    backgrounding_pending: bool,
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
            shortener: None,
            backgrounding_pending: false,
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
        let border_w = 40;
        let h_bar: String = style_tokens::BOX_H.repeat(border_w);

        // Top border: ╭── Plan ──────...
        let label = " Plan ";
        let top_after = border_w.saturating_sub(2 + label.len());
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}{}", style_tokens::BOX_TL, style_tokens::BOX_H.repeat(2)),
                border_style,
            ),
            Span::styled(
                label.to_string(),
                border_style.add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(style_tokens::BOX_H.repeat(top_after), border_style),
        ]));

        // Top padding
        lines.push(Line::from(vec![Span::styled(
            style_tokens::BOX_V.to_string(),
            border_style,
        )]));

        // Render content through markdown
        let md_lines = MarkdownRenderer::render(content);
        let prefix = format!("{}  ", style_tokens::BOX_V);
        for md_line in md_lines {
            let mut spans = vec![Span::styled(prefix.clone(), border_style)];
            spans.extend(md_line.spans);
            lines.push(Line::from(spans));
        }

        // Bottom padding
        lines.push(Line::from(vec![Span::styled(
            style_tokens::BOX_V.to_string(),
            border_style,
        )]));

        // Bottom border: ╰──────────────...
        lines.push(Line::from(vec![Span::styled(
            format!("{}{}", style_tokens::BOX_BL, h_bar),
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
                DisplayRole::User
                | DisplayRole::System
                | DisplayRole::Interrupt
                | DisplayRole::SlashCommand
                | DisplayRole::CommandResult => {
                    let rs = msg.role.style().unwrap();
                    Self::render_simple_role(&content, &rs, &mut lines);
                }
                DisplayRole::Reasoning => {
                    let md_lines =
                        MarkdownRenderer::render_muted(&content, style_tokens::THINKING_BG);
                    let thinking_style = Style::default().fg(style_tokens::THINKING_BG);
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
                            result_line.clone(),
                            Style::default().fg(style_tokens::SUBTLE),
                        ),
                    ]));
                }
            }
        } else if effective_collapsed && !tc.result_lines.is_empty() {
            let count = tc.result_lines.len();
            let verb = crate::formatters::tool_registry::lookup_tool(&tc.name).verb;
            let label = format!("  {}  {verb} {count} lines", CONTINUATION_CHAR);
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
        // read_file shows "Read 2 lines" without parentheses or "Ctrl+O" hint
        assert!(text.contains("Read 2 lines"));
        assert!(!text.contains("Ctrl+O"));
        assert!(!text.contains("("));
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
        let render_lines = widget.build_render_lines();
        let text: String = render_lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // Should show display name like "Bash ls -la" not raw "run_command"
        assert!(text.contains("Bash"));
        assert!(text.contains("ls -la"));
        assert!(text.contains("3s"));
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

    /// Test that parallel subagents show "Done" when SubagentFinished arrives before ToolFinished.
    /// This simulates the real event ordering: SubagentFinished#1,#2,#3 then ToolResult/ToolFinished.
    #[test]
    fn test_spinner_parallel_subagents_finished_before_tool() {
        use crate::widgets::nested_tool::SubagentDisplayState;

        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "Explore the codebase".into(),
            tool_call: None,
            collapsed: false,
        }];

        // Create 3 spawn_subagent ToolExecutions (still active — not finished)
        let tasks = [
            "Search for authentication code",
            "Find database models",
            "Explore API endpoints",
        ];
        let tools: Vec<ToolExecution> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let mut args = std::collections::HashMap::new();
                args.insert("task".into(), serde_json::Value::String(task.to_string()));
                args.insert(
                    "agent_type".into(),
                    serde_json::Value::String("explore".into()),
                );
                ToolExecution {
                    id: format!("t{i}"),
                    name: "spawn_subagent".into(),
                    output_lines: vec![],
                    state: crate::app::ToolState::Running,
                    elapsed_secs: 5,
                    started_at: std::time::Instant::now(),
                    tick_count: 0,
                    parent_id: None,
                    depth: 0,
                    args,
                }
            })
            .collect();

        // All 3 subagents are finished (simulating SubagentFinished arriving first)
        let subagents: Vec<SubagentDisplayState> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let mut sa =
                    SubagentDisplayState::new(format!("sa{i}"), "explore".into(), task.to_string());
                sa.parent_tool_id = Some(format!("t{i}"));
                sa.finished = true;
                sa.success = true;
                sa.tool_call_count = 3 + i;
                sa
            })
            .collect();

        let widget = ConversationWidget::new(&msgs, 0)
            .active_tools(&tools)
            .active_subagents(&subagents);
        let spinner = widget.build_spinner_lines();
        let text: String = spinner
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();

        // Each subagent should be rendered individually (no grouping)
        assert!(!text.contains("3 subagents"), "Should not group subagents");
        for task in &tasks {
            assert!(
                text.contains(task),
                "Expected task '{task}' in spinner output"
            );
        }
        // Each finished subagent should show Done individually
        assert!(text.contains("Done"));
    }

    /// Test that in-progress parallel subagents still show active tool calls (not "Done").
    #[test]
    fn test_spinner_parallel_subagents_in_progress() {
        use crate::widgets::nested_tool::{NestedToolCallState, SubagentDisplayState};

        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "Explore".into(),
            tool_call: None,
            collapsed: false,
        }];

        let tasks = ["Search auth code", "Find models"];
        let tools: Vec<ToolExecution> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let mut args = std::collections::HashMap::new();
                args.insert("task".into(), serde_json::Value::String(task.to_string()));
                args.insert(
                    "agent_type".into(),
                    serde_json::Value::String("explore".into()),
                );
                ToolExecution {
                    id: format!("t{i}"),
                    name: "spawn_subagent".into(),
                    output_lines: vec![],
                    state: crate::app::ToolState::Running,
                    elapsed_secs: 2,
                    started_at: std::time::Instant::now(),
                    tick_count: 0,
                    parent_id: None,
                    depth: 0,
                    args,
                }
            })
            .collect();

        // Subagents are NOT finished — still running with active tools
        let subagents: Vec<SubagentDisplayState> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let mut sa =
                    SubagentDisplayState::new(format!("sa{i}"), "explore".into(), task.to_string());
                sa.parent_tool_id = Some(format!("t{i}"));
                sa.active_tools.insert(
                    format!("nested_t{i}"),
                    NestedToolCallState {
                        tool_name: "read_file".into(),
                        tool_id: format!("nested_t{i}"),
                        args: Default::default(),
                        started_at: std::time::Instant::now(),
                        tick: 0,
                    },
                );
                sa
            })
            .collect();

        let widget = ConversationWidget::new(&msgs, 0)
            .active_tools(&tools)
            .active_subagents(&subagents);
        let spinner = widget.build_spinner_lines();
        let text: String = spinner
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();

        // Should NOT show "Done" — subagents are still running
        assert!(
            !text.contains("Done"),
            "Should not show 'Done' for in-progress subagents"
        );
        // Each subagent rendered individually with its active tool
        assert!(
            text.contains("Read"),
            "active tool 'Read' should appear in individual spinner lines"
        );
        // No grouping
        assert!(!text.contains("2 subagents"), "Should not group subagents");
    }

    #[test]
    fn test_render_lines_include_spinner_content() {
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
        let text: String = widget
            .build_render_lines()
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("> Do something") || text.contains("Do something"));
        assert!(text.contains("Bash"));
        assert!(text.contains("3s"));
    }

    #[test]
    fn test_snapshot_parallel_subagents_group_visible_in_tui() {
        use crate::widgets::nested_tool::SubagentDisplayState;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(60, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "Explore the repo".into(),
            tool_call: None,
            collapsed: false,
        }];

        let tasks = [
            "Search auth code",
            "Find database models",
            "Explore API routes",
            "Trace background jobs",
        ];

        let tools: Vec<ToolExecution> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let mut args = std::collections::HashMap::new();
                args.insert("task".into(), serde_json::Value::String(task.to_string()));
                args.insert(
                    "description".into(),
                    serde_json::Value::String(task.to_string()),
                );
                ToolExecution {
                    id: format!("t{i}"),
                    name: "spawn_subagent".into(),
                    output_lines: vec![],
                    state: crate::app::ToolState::Running,
                    elapsed_secs: 2,
                    started_at: std::time::Instant::now(),
                    tick_count: 0,
                    parent_id: None,
                    depth: 0,
                    args,
                }
            })
            .collect();

        let subagents: Vec<SubagentDisplayState> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let mut sa =
                    SubagentDisplayState::new(format!("sa{i}"), "explore".into(), task.to_string());
                sa.parent_tool_id = Some(format!("t{i}"));
                sa
            })
            .collect();

        terminal
            .draw(|frame| {
                let widget = ConversationWidget::new(&msgs, 0)
                    .active_tools(&tools)
                    .active_subagents(&subagents);
                frame.render_widget(widget, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let all_text = buffer_text(&buf, Rect::new(0, 0, 60, 8)).join("\n");

        // Each subagent rendered individually (no grouping)
        assert!(
            !all_text.contains("4 subagents"),
            "Should not group subagents: {all_text}"
        );
        // At least some individual agents visible in the 8-row viewport
        let visible = tasks.iter().filter(|t| all_text.contains(*t)).count();
        assert!(
            visible >= 1,
            "At least one subagent should be visible: {all_text}"
        );
    }

    #[test]
    fn test_reasoning_message_visible() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let msgs = vec![
            DisplayMessage {
                role: DisplayRole::Reasoning,
                content:
                    "Let me think step by step.\nFirst analyze the problem.\nThen find a solution."
                        .into(),
                tool_call: None,
                collapsed: false,
            },
            DisplayMessage {
                role: DisplayRole::Assistant,
                content: "The answer is 42.".into(),
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
        let rows = buffer_text(&buf, Rect::new(0, 0, 80, 24));
        let all_text = rows.join("\n");

        // Thinking content should be visible
        assert!(
            all_text.contains("think step by step"),
            "Reasoning content missing from render. Rows:\n{}",
            rows.iter()
                .enumerate()
                .map(|(i, r)| format!("  [{i:2}] {r:?}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        // Assistant content should also be visible
        assert!(
            all_text.contains("answer is 42"),
            "Assistant content missing from render"
        );
        // Check for thinking icon
        assert!(
            all_text.contains('⟡'),
            "Missing ⟡ thinking icon. Rows:\n{}",
            rows.iter()
                .enumerate()
                .map(|(i, r)| format!("  [{i:2}] {r:?}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
