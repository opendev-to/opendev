//! Task progress display widget.
//!
//! Mirrors the Python `TaskProgressDisplay` — shows an animated spinner with
//! task description, elapsed time, and token usage during agent/tool execution.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Widget,
};

use super::spinner::SpinnerState;
use crate::formatters::style_tokens;

/// Task progress display data.
#[derive(Debug, Clone)]
pub struct TaskProgress {
    /// Task description (e.g., "Thinking...", "Running bash").
    pub description: String,
    /// Elapsed seconds since task started.
    pub elapsed_secs: u64,
    /// Token usage display string (e.g., "1.2k tokens").
    pub token_display: Option<String>,
    /// Whether the task was interrupted.
    pub interrupted: bool,
    /// Wall-clock start time for accurate elapsed calculation.
    pub started_at: std::time::Instant,
}

/// Widget that renders task progress with animated spinner.
pub struct TaskProgressWidget<'a> {
    progress: &'a TaskProgress,
    spinner_char: char,
}

impl<'a> TaskProgressWidget<'a> {
    pub fn new(progress: &'a TaskProgress, spinner_state: &SpinnerState) -> Self {
        Self {
            progress,
            spinner_char: spinner_state.current(),
        }
    }
}

impl Widget for TaskProgressWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let mut spans: Vec<Span> = Vec::new();

        // Spinner character
        spans.push(Span::styled(
            format!("{} ", self.spinner_char),
            Style::default().fg(style_tokens::BLUE_BRIGHT),
        ));

        // Task description
        spans.push(Span::styled(
            format!("{}... ", self.progress.description),
            Style::default().fg(style_tokens::SUBTLE),
        ));

        // Info section: esc to interrupt · Xs · token_display
        let mut info_parts = Vec::new();
        info_parts.push("esc to interrupt".to_string());
        info_parts.push(format!("{}s", self.progress.elapsed_secs));

        if let Some(ref token_display) = self.progress.token_display {
            info_parts.push(token_display.clone());
        }

        let info_str = info_parts.join(" \u{00b7} "); // middle dot separator
        spans.push(Span::styled(
            info_str,
            Style::default().fg(style_tokens::SUBTLE),
        ));

        let line = Line::from(spans);
        buf.set_line(area.left(), area.top(), &line, area.width);
    }
}

/// Format a final status line after task completion.
///
/// Returns a formatted string like "⏺ completed in 5s (1.2k tokens)".
pub fn format_final_status(progress: &TaskProgress) -> String {
    let symbol = if progress.interrupted {
        "\u{23f9}" // ⏹
    } else {
        "\u{23fa}" // ⏺
    };

    let status = if progress.interrupted {
        "interrupted"
    } else {
        "completed"
    };

    let mut parts = vec![format!("{status} in {}s", progress.elapsed_secs)];
    if let Some(ref token_display) = progress.token_display {
        parts.push(token_display.clone());
    }

    format!("{symbol} {}", parts.join(", "))
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
