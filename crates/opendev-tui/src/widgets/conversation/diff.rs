//! Diff parsing and rendering for edit/write tool output.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::formatters::style_tokens;

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

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
