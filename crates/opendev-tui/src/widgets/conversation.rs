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
    widgets::{Paragraph, Widget, Wrap},
};

use crate::app::{DisplayMessage, DisplayRole, DisplayToolCall, ToolExecution};
use crate::formatters::display::strip_system_reminders;
use crate::formatters::markdown::MarkdownRenderer;
use crate::formatters::style_tokens::{self, Indent};
use crate::formatters::tool_registry::{categorize_tool, format_tool_call_display, tool_color};
use crate::widgets::progress::TaskProgress;
use crate::widgets::spinner::{COMPLETED_CHAR, CONTINUATION_CHAR, SPINNER_FRAMES};

/// Widget that renders the conversation log.
pub struct ConversationWidget<'a> {
    messages: &'a [DisplayMessage],
    scroll_offset: u16,
    terminal_width: u16,
    version: &'a str,
    working_dir: &'a str,
    mode: &'a str,
    /// Active tool executions (shown as inline spinners).
    active_tools: &'a [ToolExecution],
    /// Task progress (thinking state, shown when no active tools).
    task_progress: Option<&'a TaskProgress>,
    /// Pre-computed spinner character for the current frame.
    spinner_char: char,
    /// Pre-built cached lines for the static message portion (if available).
    cached_lines: Option<&'a [Line<'static>]>,
}

impl<'a> ConversationWidget<'a> {
    pub fn new(messages: &'a [DisplayMessage], scroll_offset: u16) -> Self {
        Self {
            messages,
            scroll_offset,
            terminal_width: 80,
            version: "0.1.0",
            working_dir: ".",
            mode: "NORMAL",
            active_tools: &[],
            task_progress: None,
            spinner_char: SPINNER_FRAMES[0],
            cached_lines: None,
        }
    }

    pub fn terminal_width(mut self, width: u16) -> Self {
        self.terminal_width = width;
        self
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

    /// Supply pre-built cached lines for the static message portion.
    /// When set, `build_lines()` is skipped and these lines are used directly,
    /// with dynamic spinner/progress lines still built fresh each frame.
    pub fn cached_lines(mut self, lines: &'a [Line<'static>]) -> Self {
        self.cached_lines = Some(lines);
        self
    }

    /// Build styled lines from messages.
    fn build_lines(&self) -> Vec<Line<'a>> {
        let mut lines: Vec<Line> = Vec::new();

        if self.messages.is_empty() {
            // Welcome panel is now rendered as a separate widget (WelcomePanelWidget)
            return lines;
        }

        for msg in self.messages {
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
                DisplayRole::User => {
                    let content_lines: Vec<&str> = content.lines().collect();
                    for (i, content_line) in content_lines.iter().enumerate() {
                        if i == 0 {
                            lines.push(Line::from(vec![
                                Span::styled(
                                    "> ".to_string(),
                                    Style::default()
                                        .fg(style_tokens::ACCENT)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(
                                    content_line.to_string(),
                                    Style::default().fg(style_tokens::PRIMARY),
                                ),
                            ]));
                        } else {
                            lines.push(Line::from(vec![
                                Span::raw(Indent::CONT),
                                Span::styled(
                                    content_line.to_string(),
                                    Style::default().fg(style_tokens::PRIMARY),
                                ),
                            ]));
                        }
                    }
                }
                DisplayRole::System => {
                    let content_lines: Vec<&str> = content.lines().collect();
                    for (i, content_line) in content_lines.iter().enumerate() {
                        if i == 0 {
                            lines.push(Line::from(vec![
                                Span::styled(
                                    "! ".to_string(),
                                    Style::default()
                                        .fg(style_tokens::WARNING)
                                        .add_modifier(Modifier::ITALIC),
                                ),
                                Span::styled(
                                    content_line.to_string(),
                                    Style::default().fg(style_tokens::SUBTLE),
                                ),
                            ]));
                        } else {
                            lines.push(Line::from(vec![
                                Span::raw(Indent::CONT),
                                Span::styled(
                                    content_line.to_string(),
                                    Style::default().fg(style_tokens::SUBTLE),
                                ),
                            ]));
                        }
                    }
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
                let tool_line = format_tool_call(tc);
                lines.push(tool_line);

                // Collapsible result lines
                if !tc.collapsed && !tc.result_lines.is_empty() {
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
                } else if tc.collapsed && !tc.result_lines.is_empty() {
                    // Show collapsed indicator
                    let count = tc.result_lines.len();
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  {}  ({count} lines collapsed, press Ctrl+O to expand)",
                            CONTINUATION_CHAR
                        ),
                        Style::default()
                            .fg(style_tokens::SUBTLE)
                            .add_modifier(Modifier::ITALIC),
                    )));
                }

                // Nested tool calls (from subagent execution)
                for nested in &tc.nested_calls {
                    let nested_line = format_nested_tool_call(nested, 1);
                    lines.push(nested_line);
                }
            }

            // Blank line between messages
            lines.push(Line::from(""));
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

        if !active_unfinished.is_empty() {
            for tool in &active_unfinished {
                let category = categorize_tool(&tool.name);
                let name_color = tool_color(category);
                let frame_idx = tool.tick_count % SPINNER_FRAMES.len();
                let spinner = SPINNER_FRAMES[frame_idx];
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{spinner} "),
                        Style::default().fg(style_tokens::BLUE_BRIGHT),
                    ),
                    Span::styled(
                        tool.name.clone(),
                        Style::default().fg(name_color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" ({}s)", tool.elapsed_secs),
                        Style::default().fg(style_tokens::GREY),
                    ),
                ]));
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
fn format_tool_call(tc: &DisplayToolCall) -> Line<'static> {
    let category = categorize_tool(&tc.name);
    let color = tool_color(category);

    let (icon, icon_color) = if tc.success {
        (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
    } else {
        (COMPLETED_CHAR, style_tokens::ERROR)
    };

    let display = format_tool_call_display(&tc.name, &tc.arguments);

    Line::from(vec![
        Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
        Span::styled(
            display,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Format a nested tool call with tree indent.
fn format_nested_tool_call(tc: &DisplayToolCall, depth: usize) -> Line<'static> {
    let indent = Indent::for_depth(depth);
    let category = categorize_tool(&tc.name);
    let color = tool_color(category);

    let (icon, icon_color) = if tc.success {
        (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
    } else {
        (COMPLETED_CHAR, style_tokens::ERROR)
    };

    let display = format_tool_call_display(&tc.name, &tc.arguments);

    Line::from(vec![
        Span::styled(
            format!("{indent}\u{2514}\u{2500} "),
            Style::default().fg(style_tokens::SUBTLE),
        ),
        Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
        Span::styled(display, Style::default().fg(color)),
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

        // Scroll position indicator when scrolled up
        if self.scroll_offset > 0 && max_scroll > 0 {
            let pct = ((max_scroll - actual_scroll) as f64 / max_scroll as f64 * 100.0) as u16;
            let indicator = format!(" \u{2191} {}% ", pct);
            let x = area.right().saturating_sub(indicator.len() as u16);
            buf.set_string(
                x,
                area.y,
                &indicator,
                Style::default().fg(style_tokens::SUBTLE),
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
        assert!(text.contains("collapsed"));
    }

    #[test]
    fn test_spinner_active_tools() {
        let msgs = vec![DisplayMessage {
            role: DisplayRole::User,
            content: "Do something".into(),
            tool_call: None,
            collapsed: false,
        }];
        let tools = vec![ToolExecution {
            id: "t1".into(),
            name: "bash".into(),
            output_lines: vec![],
            state: crate::app::ToolState::Running,
            elapsed_secs: 3,
            started_at: std::time::Instant::now(),
            tick_count: 0,
            parent_id: None,
            depth: 0,
            args: Default::default(),
        }];
        let widget = ConversationWidget::new(&msgs, 0).active_tools(&tools);
        // Spinner is now built separately from message lines
        let spinner = widget.build_spinner_lines();
        let text: String = spinner
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("bash"));
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
        // Active tools shown, not thinking
        assert!(text.contains("read_file"));
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
        // Tool calls now use format_tool_call_display: Spawn(subagent), Read(file)
        assert!(text.contains("Spawn"));
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
        let first_row: String = (0..80u16)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect();

        // When scrolled up, the first row should contain the scroll percentage indicator
        assert!(
            first_row.contains('%'),
            "Expected scroll indicator with % on first row when scrolled, got: {first_row:?}"
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
}
