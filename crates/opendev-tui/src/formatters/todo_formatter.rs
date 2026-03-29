//! Todo tool formatter — produces concise 1-line summaries for todo operations.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::base::{FormattedOutput, ToolFormatter};
use super::style_tokens;

/// Formatter for todo tool results.
pub struct TodoFormatter;

/// Summarize a todo tool result into a single display line.
pub fn summarize_todo_result(tool_name: &str, output: &str) -> String {
    match tool_name {
        "write_todos" => {
            let count = output
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    t.starts_with("[todo]") || t.starts_with("[doing]") || t.starts_with("[done]")
                })
                .count();
            if count == 0 {
                let fallback = output
                    .lines()
                    .filter(|l| !l.trim().is_empty() && !l.starts_with("Todos"))
                    .count();
                if fallback == 1 {
                    "Created 1 todo".to_string()
                } else {
                    format!("Created {fallback} todos")
                }
            } else if count == 1 {
                "Created 1 todo".to_string()
            } else {
                format!("Created {count} todos")
            }
        }
        "list_todos" => {
            if output.contains("No todos") {
                "No todos".to_string()
            } else {
                let mut done = 0usize;
                let mut doing = 0usize;
                let mut pending = 0usize;
                for line in output.lines() {
                    let t = line.trim();
                    if t.starts_with("[done]") {
                        done += 1;
                    } else if t.starts_with("[doing]") {
                        doing += 1;
                    } else if t.starts_with("[todo]") {
                        pending += 1;
                    }
                }
                let total = done + doing + pending;
                if total == 0 {
                    "No todos".to_string()
                } else {
                    let mut parts = Vec::new();
                    if doing > 0 {
                        parts.push(format!("{doing} active"));
                    }
                    if done > 0 {
                        parts.push(format!("{done} done"));
                    }
                    if pending > 0 {
                        parts.push(format!("{pending} pending"));
                    }
                    format!("{total} todos ({})", parts.join(", "))
                }
            }
        }
        "update_todo" => {
            for line in output.lines() {
                let t = line.trim();
                if t.starts_with("[doing]")
                    && let Some(title) = extract_todo_title(t, "[doing]")
                {
                    return format!("\u{25b6} In progress: {title}");
                }
            }
            "Updated todo".to_string()
        }
        "complete_todo" => {
            for line in output.lines() {
                let t = line.trim();
                if t.starts_with("[done]")
                    && let Some(title) = extract_todo_title(t, "[done]")
                {
                    return format!("\u{2714} Completed: {title}");
                }
            }
            "Completed todo".to_string()
        }
        "clear_todos" => "All todos cleared".to_string(),
        _ => {
            let first_line = output.lines().next().unwrap_or("").trim();
            if first_line.len() <= 60 {
                first_line.to_string()
            } else {
                format!("{}...", &first_line[..57])
            }
        }
    }
}

fn extract_todo_title(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?.trim();
    if let Some(dot_pos) = rest.find(". ") {
        let title = rest[dot_pos + 2..].trim();
        if !title.is_empty() {
            return Some(title.to_string());
        }
    }
    None
}

impl ToolFormatter for TodoFormatter {
    fn format<'a>(&self, tool_name: &str, output: &str) -> FormattedOutput<'a> {
        let summary = summarize_todo_result(tool_name, output);

        let header = Line::from(vec![
            Span::styled(
                "  \u{2714} ".to_string(),
                Style::default().fg(style_tokens::SUCCESS),
            ),
            Span::styled(summary, Style::default().fg(style_tokens::PRIMARY)),
        ]);

        FormattedOutput {
            header,
            body: Vec::new(),
            footer: None,
        }
    }

    fn handles(&self, tool_name: &str) -> bool {
        matches!(
            tool_name,
            "write_todos" | "update_todo" | "complete_todo" | "list_todos" | "clear_todos" | "todo"
        )
    }
}

#[cfg(test)]
#[path = "todo_formatter_tests.rs"]
mod tests;
