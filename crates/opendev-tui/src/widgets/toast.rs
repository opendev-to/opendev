//! Toast notification widget for transient feedback messages.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

/// Toast notification severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    fn color(self) -> Color {
        match self {
            ToastLevel::Info => Color::Rgb(0, 255, 255),     // cyan
            ToastLevel::Success => Color::Rgb(0, 200, 100),  // green
            ToastLevel::Warning => Color::Rgb(255, 200, 50), // yellow
            ToastLevel::Error => Color::Rgb(255, 80, 80),    // red
        }
    }

    fn icon(self) -> &'static str {
        match self {
            ToastLevel::Info => "\u{2139}",    // ℹ
            ToastLevel::Success => "\u{2713}", // ✓
            ToastLevel::Warning => "\u{26a0}", // ⚠
            ToastLevel::Error => "\u{2717}",   // ✗
        }
    }
}

/// A single toast notification.
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created_at: Instant,
    pub duration: Duration,
}

impl Toast {
    pub fn new(message: impl Into<String>, level: ToastLevel) -> Self {
        Self {
            message: message.into(),
            level,
            created_at: Instant::now(),
            duration: Duration::from_secs(3),
        }
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }

    /// Returns opacity factor (1.0 = fully visible, 0.0 = gone).
    /// Fades out during the last 500ms.
    pub fn opacity(&self) -> f64 {
        let elapsed = self.created_at.elapsed();
        if elapsed >= self.duration {
            return 0.0;
        }
        let remaining = self.duration - elapsed;
        if remaining < Duration::from_millis(500) {
            remaining.as_millis() as f64 / 500.0
        } else {
            1.0
        }
    }
}

/// Renders toast notifications in the top-right corner.
pub struct ToastWidget<'a> {
    toasts: &'a [Toast],
}

impl<'a> ToastWidget<'a> {
    pub fn new(toasts: &'a [Toast]) -> Self {
        Self { toasts }
    }
}

impl ratatui::widgets::Widget for ToastWidget<'_> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        if self.toasts.is_empty() {
            return;
        }

        let max_toasts = 3;
        let toast_width = 50u16.min(area.width.saturating_sub(4));
        let mut y_offset = 1u16; // Start 1 row from top

        for toast in self.toasts.iter().rev().take(max_toasts) {
            if y_offset + 3 > area.height {
                break;
            }

            let toast_area = Rect {
                x: area.width.saturating_sub(toast_width + 2),
                y: area.y + y_offset,
                width: toast_width,
                height: 3,
            };

            let color = toast.level.color();
            let icon = toast.level.icon();

            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(color));

            let line = Line::from(vec![
                Span::styled(
                    format!(" {icon} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(toast.message.clone(), Style::default().fg(Color::White)),
            ]);

            let paragraph = Paragraph::new(vec![line]).block(block);
            // Clear area first
            ratatui::widgets::Widget::render(ratatui::widgets::Clear, toast_area, buf);
            ratatui::widgets::Widget::render(paragraph, toast_area, buf);

            y_offset += 3;
        }
    }
}

#[cfg(test)]
#[path = "toast_tests.rs"]
mod tests;
