//! Status bar widget showing model, tokens, mode, autonomy, git branch, MCP, cost.
//!
//! Displays:
//! - Autonomy level (Manual/Semi-Auto/Auto) with Ctrl+Shift+A hint
//! - Repo path and git branch
//! - MCP server status
//! - Session cost
//! - Context window remaining percentage

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use unicode_width::UnicodeWidthStr;

use crate::app::{AutonomyLevel, OperationMode, ReasoningLevel};
use crate::formatters::style_tokens;

/// Bottom status bar widget.
#[allow(dead_code)]
pub struct StatusBarWidget<'a> {
    model: &'a str,
    working_dir: &'a str,
    git_branch: Option<&'a str>,
    tokens_used: u64,
    tokens_limit: u64,
    mode: OperationMode,
    // New fields from Python StatusBar
    autonomy: AutonomyLevel,
    context_usage_pct: f64,
    session_cost: f64,
    mcp_status: Option<(usize, usize)>,
    mcp_has_errors: bool,
    background_tasks: usize,
    file_changes: Option<(usize, u64, u64)>,
    reasoning_level: Option<ReasoningLevel>,
    spinner_char: Option<char>,
    last_completion: Option<String>,
    session_id: Option<&'a str>,
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(
        model: &'a str,
        working_dir: &'a str,
        git_branch: Option<&'a str>,
        tokens_used: u64,
        tokens_limit: u64,
        mode: OperationMode,
    ) -> Self {
        Self {
            model,
            working_dir,
            git_branch,
            tokens_used,
            tokens_limit,
            mode,
            autonomy: AutonomyLevel::SemiAuto,
            context_usage_pct: 0.0,
            session_cost: 0.0,
            mcp_status: None,
            mcp_has_errors: false,
            background_tasks: 0,
            file_changes: None,
            reasoning_level: None,
            spinner_char: None,
            last_completion: None,
            session_id: None,
        }
    }

    pub fn autonomy(mut self, autonomy: AutonomyLevel) -> Self {
        self.autonomy = autonomy;
        self
    }

    pub fn context_usage_pct(mut self, pct: f64) -> Self {
        self.context_usage_pct = pct;
        self
    }

    pub fn session_cost(mut self, cost: f64) -> Self {
        self.session_cost = cost;
        self
    }

    pub fn mcp_status(mut self, status: Option<(usize, usize)>, has_errors: bool) -> Self {
        self.mcp_status = status;
        self.mcp_has_errors = has_errors;
        self
    }

    pub fn background_tasks(mut self, count: usize) -> Self {
        self.background_tasks = count;
        self
    }

    pub fn file_changes(mut self, changes: Option<(usize, u64, u64)>) -> Self {
        self.file_changes = changes;
        self
    }

    pub fn reasoning_level(mut self, level: ReasoningLevel) -> Self {
        self.reasoning_level = Some(level);
        self
    }

    pub fn spinner_char(mut self, ch: Option<char>) -> Self {
        self.spinner_char = ch;
        self
    }

    pub fn last_completion(mut self, info: Option<String>) -> Self {
        self.last_completion = info;
        self
    }

    pub fn session_id(mut self, id: Option<&'a str>) -> Self {
        self.session_id = id;
        self
    }

    #[allow(dead_code)]
    fn format_tokens(n: u64) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}k", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }

        let mut spans: Vec<Span> = Vec::new();

        // Model name (first item)
        spans.push(Span::styled(
            format!("\u{25C6} {}", self.model),
            Style::default()
                .fg(style_tokens::CYAN)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            "  \u{2502}  ",
            Style::default().fg(style_tokens::GREY),
        ));

        // Autonomy level
        let autonomy_color = match self.autonomy {
            AutonomyLevel::Manual => style_tokens::ORANGE_CAUTION,
            AutonomyLevel::SemiAuto => style_tokens::CYAN,
            AutonomyLevel::Auto => style_tokens::GREEN_BRIGHT,
        };
        spans.push(Span::styled(
            "Autonomy: ",
            Style::default().fg(style_tokens::GREY),
        ));
        spans.push(Span::styled(
            self.autonomy.to_string(),
            Style::default()
                .fg(autonomy_color)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            " (Ctrl+Shift+A)",
            Style::default().fg(style_tokens::GREY),
        ));

        // Reasoning effort level
        if let Some(level) = self.reasoning_level {
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
            spans.push(Span::styled(
                "Thinking: ",
                Style::default().fg(style_tokens::GREY),
            ));
            let thinking_color = match level {
                ReasoningLevel::Off => style_tokens::GREY,
                ReasoningLevel::Low => style_tokens::CYAN,
                ReasoningLevel::Medium => style_tokens::GREEN_BRIGHT,
                ReasoningLevel::High => style_tokens::GOLD,
            };
            spans.push(Span::styled(
                level.to_string(),
                Style::default()
                    .fg(thinking_color)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                " (Ctrl+Shift+T)",
                Style::default().fg(style_tokens::GREY),
            ));
        }

        // Repo info (path + git branch)
        let repo_display = self.build_repo_display();
        if !repo_display.is_empty() {
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
            spans.push(Span::styled(
                repo_display,
                Style::default().fg(style_tokens::BLUE_PATH),
            ));
        }

        // MCP status (only when servers configured)
        if let Some((connected, total)) = self.mcp_status {
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
            let mcp_label = format!("MCP: {connected}/{total}");
            let mcp_color = if self.mcp_has_errors {
                style_tokens::ORANGE
            } else if connected < total {
                style_tokens::GOLD
            } else {
                style_tokens::GREEN_LIGHT
            };
            spans.push(Span::styled(
                mcp_label,
                Style::default().fg(mcp_color).add_modifier(Modifier::BOLD),
            ));
        }

        // Background tasks
        if self.background_tasks > 0 {
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
            // Prepend spinner when tasks are running
            if let Some(ch) = self.spinner_char {
                spans.push(Span::styled(
                    format!("{ch} "),
                    Style::default()
                        .fg(style_tokens::CYAN)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            spans.push(Span::styled(
                format!("\u{2699} {}", self.background_tasks),
                Style::default().fg(style_tokens::BLUE_TASK),
            ));
            spans.push(Span::styled(
                " (Ctrl+P)",
                Style::default().fg(style_tokens::GREY),
            ));
        }

        // Completion flash
        if let Some(ref info) = self.last_completion {
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
            spans.push(Span::styled(
                info.clone(),
                Style::default()
                    .fg(style_tokens::GREEN_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // File changes summary
        if let Some((files, additions, deletions)) = self.file_changes {
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
            spans.push(Span::styled(
                format!("{files} file{}", if files == 1 { "" } else { "s" }),
                Style::default()
                    .fg(style_tokens::BLUE_PATH)
                    .add_modifier(Modifier::BOLD),
            ));
            if additions > 0 || deletions > 0 {
                spans.push(Span::styled(" ", Style::default()));
                if additions > 0 {
                    spans.push(Span::styled(
                        format!("+{additions}"),
                        Style::default()
                            .fg(style_tokens::GREEN_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                if additions > 0 && deletions > 0 {
                    spans.push(Span::styled(" ", Style::default()));
                }
                if deletions > 0 {
                    spans.push(Span::styled(
                        format!("-{deletions}"),
                        Style::default()
                            .fg(style_tokens::ORANGE)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }

        // Right-aligned section: cost + context remaining
        let context_left = (100.0 - self.context_usage_pct).max(0.0);
        let pct_str = format!("{:>5}", format!("{context_left:.1}"));
        let pct_color = if context_left > 50.0 {
            style_tokens::GREEN_LIGHT
        } else if context_left > 25.0 {
            style_tokens::GOLD
        } else {
            style_tokens::ORANGE
        };

        let cost_str = if self.session_cost > 0.0 {
            if self.session_cost < 0.01 {
                format!("{:>7}", format!("${:.4}", self.session_cost))
            } else {
                format!("{:>7}", format!("${:.2}", self.session_cost))
            }
        } else {
            String::new()
        };

        // Build right-side spans
        let mut right_spans: Vec<Span> = Vec::new();

        // Session ID (right-aligned, subtle)
        if let Some(sid) = self.session_id {
            right_spans.push(Span::styled(
                format!("#{sid}"),
                Style::default().fg(style_tokens::GREY),
            ));
            right_spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
        }

        if !cost_str.is_empty() {
            right_spans.push(Span::styled(
                "Cost ",
                Style::default().fg(style_tokens::GREY),
            ));
            right_spans.push(Span::styled(
                cost_str,
                Style::default()
                    .fg(style_tokens::CYAN)
                    .add_modifier(Modifier::BOLD),
            ));
            right_spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
        }
        right_spans.push(Span::styled(
            format!("Context left {pct_str}%"),
            Style::default().fg(pct_color).add_modifier(Modifier::BOLD),
        ));

        let right_len: usize = right_spans.iter().map(|s| s.content.width()).sum();
        let available_width = area.width as usize;

        // Determine the row to render on
        let row = if area.height >= 2 {
            // Draw thin border on the first row
            let border_line: String = "\u{2500}".repeat(available_width);
            buf.set_string(
                area.left(),
                area.top(),
                &border_line,
                Style::default().fg(style_tokens::BORDER),
            );
            area.top() + 1
        } else {
            area.top()
        };

        // Render right spans at fixed position (anchored to right edge)
        let right_start = area.right().saturating_sub(right_len as u16);
        let right_line = Line::from(right_spans);
        buf.set_line(right_start, row, &right_line, right_len as u16);

        // Render left spans, truncated so they don't overlap the right section
        let left_max_width = (right_start as usize).saturating_sub(area.left() as usize + 2);
        let left_line = Line::from(spans);
        buf.set_line(area.left(), row, &left_line, left_max_width as u16);
    }
}

impl StatusBarWidget<'_> {
    /// Build repo display string with path and git branch.
    fn build_repo_display(&self) -> String {
        if self.working_dir.is_empty() || self.working_dir == "." {
            return String::new();
        }

        let shortener = crate::formatters::PathShortener::default();
        let dir_display = shortener.shorten_display(self.working_dir);
        match self.git_branch {
            Some(branch) => format!("{dir_display} ({branch})"),
            None => dir_display,
        }
    }
}

#[cfg(test)]
#[path = "status_bar_tests.rs"]
mod tests;
