use super::TodoStatus;

/// Map status alias strings to `TodoStatus`.
///
/// Accepts: `pending`, `todo`, `in_progress`, `doing`, `in-progress`,
/// `completed`, `done`, `complete`.
pub fn parse_status(s: &str) -> Option<TodoStatus> {
    match s.to_lowercase().trim() {
        "pending" | "todo" => Some(TodoStatus::Pending),
        "in_progress" | "doing" | "in-progress" | "in progress" => Some(TodoStatus::InProgress),
        "completed" | "done" | "complete" => Some(TodoStatus::Completed),
        _ => None,
    }
}

/// Strip basic markdown formatting from text (bold, italic, code).
pub fn strip_markdown(text: &str) -> String {
    text.replace("**", "")
        .replace("__", "")
        .replace('*', "")
        .replace('_', " ")
        .replace('`', "")
        .replace("~~", "")
}

/// Parse plan markdown content and extract numbered implementation steps.
///
/// First looks for a section header like `## Implementation Steps` or `## Steps`,
/// then extracts numbered list items from that section. If no such section exists,
/// falls back to extracting all numbered items from the entire document.
pub fn parse_plan_steps(plan_content: &str) -> Vec<String> {
    // First try: section-aware extraction
    let mut steps = Vec::new();
    let mut in_steps_section = false;

    for line in plan_content.lines() {
        let trimmed = line.trim();

        // Detect steps section header
        if trimmed.starts_with("## Implementation Steps")
            || trimmed.starts_with("## Steps")
            || trimmed.starts_with("## implementation steps")
        {
            in_steps_section = true;
            continue;
        }

        // End of section on next header
        if in_steps_section && trimmed.starts_with("## ") {
            break;
        }

        // Extract numbered items
        if in_steps_section
            && let Some(text) = extract_numbered_step(trimmed)
            && !text.is_empty()
        {
            steps.push(text);
        }
    }

    // Fallback: if no section header found, extract all numbered items
    if steps.is_empty() {
        for line in plan_content.lines() {
            let trimmed = line.trim();
            // Skip markdown headers themselves
            if trimmed.starts_with('#') {
                continue;
            }
            if let Some(text) = extract_numbered_step(trimmed)
                && !text.is_empty()
            {
                steps.push(text);
            }
        }
    }

    steps
}

/// Extract the text from a numbered list item.
///
/// Handles formats like:
/// - `1. Step text`
/// - `1) Step text`
/// - `1 - Step text`
fn extract_numbered_step(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Check if line starts with a digit
    let mut chars = line.chars();
    let first = chars.next()?;
    if !first.is_ascii_digit() {
        return None;
    }

    // Skip remaining digits
    let rest: String = chars.collect();
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());

    // Check for separator (. or ) or -)
    #[allow(clippy::question_mark)]
    let rest = if let Some(s) = rest.strip_prefix(". ") {
        s
    } else if let Some(s) = rest.strip_prefix(") ") {
        s
    } else if let Some(s) = rest.strip_prefix(" - ") {
        s
    } else {
        return None;
    };

    let text = rest.trim();
    if text.is_empty() {
        None
    } else {
        // Strip markdown bold/emphasis markers for cleaner titles
        let text = text.replace("**", "").replace("__", "");
        Some(text)
    }
}

#[cfg(test)]
#[path = "parsing_tests.rs"]
mod tests;
