//! User input/prompt widget.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::formatters::style_tokens;

/// Widget for the user input area.
pub struct InputWidget<'a> {
    buffer: &'a str,
    cursor: usize,
    agent_active: bool,
    mode: &'a str,
    pending_count: usize,
}

impl<'a> InputWidget<'a> {
    pub fn new(
        buffer: &'a str,
        cursor: usize,
        agent_active: bool,
        mode: &'a str,
        pending_count: usize,
    ) -> Self {
        Self {
            buffer,
            cursor,
            agent_active,
            mode,
            pending_count,
        }
    }
}

impl Widget for InputWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 {
            return;
        }

        let accent = if self.mode == "PLAN" {
            style_tokens::GREEN_LIGHT
        } else {
            style_tokens::ACCENT
        };

        let placeholder = if self.pending_count > 0 {
            format!(
                "{} message{} queued (ESC to cancel)",
                self.pending_count,
                if self.pending_count == 1 { "" } else { "s" }
            )
        } else if self.agent_active {
            "Agent is thinking... (ESC to interrupt)".to_owned()
        } else {
            "Type a message...".to_owned()
        };

        // Row 0: separator line with embedded mode indicator
        // e.g. "── Normal (Shift+Tab) ──────────"
        let mode_label = match self.mode {
            "NORMAL" => "Normal",
            "PLAN" => "Plan",
            other => other,
        };
        let mode_text = format!(" {mode_label} ");
        let hint_text = "(Shift+Tab) ";
        let prefix_dashes = 2; // "── " before mode label
        let used = prefix_dashes + mode_text.len() + hint_text.len();
        let remaining_dashes = (area.width as usize).saturating_sub(used);

        let sep_style = Style::default().fg(accent);
        let sep_line = Line::from(vec![
            Span::styled("── ", sep_style),
            Span::styled(
                mode_text,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(hint_text, Style::default().fg(style_tokens::GREY)),
            Span::styled("─".repeat(remaining_dashes), sep_style),
        ]);
        buf.set_line(area.left(), area.top(), &sep_line, area.width);

        // Rows below separator: multiline input
        let text_height = area.height.saturating_sub(1);
        if text_height == 0 {
            return;
        }
        let text_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: text_height,
        };

        if self.buffer.is_empty() {
            let prefix = Span::styled(
                "> ".to_string(),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            );
            let content = vec![
                prefix,
                Span::styled(placeholder, Style::default().fg(style_tokens::SUBTLE)),
            ];
            Paragraph::new(Line::from(content)).render(text_area, buf);
        } else {
            // Split buffer into lines and render each with proper prefix
            let input_lines: Vec<&str> = self.buffer.split('\n').collect();

            // Compute which line and column the cursor is on
            let mut cursor_line = 0;
            let mut cursor_col = 0;
            let mut pos = 0;
            for (i, line) in input_lines.iter().enumerate() {
                if self.cursor <= pos + line.len() {
                    cursor_line = i;
                    cursor_col = self.cursor - pos;
                    break;
                }
                pos += line.len() + 1; // +1 for '\n'
                if i == input_lines.len() - 1 {
                    cursor_line = i;
                    cursor_col = line.len();
                }
            }

            let prefix_style = Style::default().fg(accent).add_modifier(Modifier::BOLD);
            let cursor_style = Style::default().fg(Color::Black).bg(Color::White);

            for (i, line_text) in input_lines.iter().enumerate() {
                if i as u16 >= text_height {
                    break;
                }
                let row = text_area.y + i as u16;
                let pfx = if i == 0 { "> " } else { "  " };

                if i == cursor_line {
                    let before = &line_text[..cursor_col];
                    let (cursor_char, after) = if cursor_col < line_text.len() {
                        // Find the end of the current char (next char boundary)
                        let ch = line_text[cursor_col..].chars().next().unwrap();
                        let end = cursor_col + ch.len_utf8();
                        (&line_text[cursor_col..end], &line_text[end..])
                    } else {
                        (" ", "")
                    };
                    let spans = Line::from(vec![
                        Span::styled(pfx, prefix_style),
                        Span::raw(before.to_string()),
                        Span::styled(cursor_char.to_string(), cursor_style),
                        Span::raw(after.to_string()),
                    ]);
                    buf.set_line(text_area.x, row, &spans, text_area.width);
                } else {
                    let spans = Line::from(vec![
                        Span::styled(pfx, prefix_style),
                        Span::raw(line_text.to_string()),
                    ]);
                    buf.set_line(text_area.x, row, &spans, text_area.width);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_widget_creation() {
        let _widget = InputWidget::new("hello", 3, false, "NORMAL", 0);
    }

    #[test]
    fn test_input_widget_empty() {
        let _widget = InputWidget::new("", 0, false, "NORMAL", 0);
    }

    #[test]
    fn test_input_widget_agent_active() {
        let _widget = InputWidget::new("", 0, true, "NORMAL", 0);
    }

    #[test]
    fn test_queue_indicator_placeholder() {
        // Verify the widget renders queue count in placeholder
        let area = Rect::new(0, 0, 60, 3);
        let mut buf = Buffer::empty(area);

        let widget = InputWidget::new("", 0, true, "NORMAL", 2);
        widget.render(area, &mut buf);

        // Check that "2 messages queued" appears in the rendered buffer
        let rendered: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 1))
                    .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        assert!(
            rendered.contains("2 messages queued"),
            "Expected '2 messages queued' in rendered output, got: {rendered:?}"
        );
    }

    #[test]
    fn test_queue_indicator_single_message() {
        let area = Rect::new(0, 0, 60, 3);
        let mut buf = Buffer::empty(area);

        let widget = InputWidget::new("", 0, true, "NORMAL", 1);
        widget.render(area, &mut buf);

        let rendered: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 1))
                    .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        assert!(
            rendered.contains("1 message queued"),
            "Expected '1 message queued' in rendered output, got: {rendered:?}"
        );
        // Should NOT say "messages" (plural)
        assert!(
            !rendered.contains("1 messages"),
            "Should use singular 'message' for count=1"
        );
    }
}
