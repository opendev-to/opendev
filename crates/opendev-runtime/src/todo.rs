//! Todo tracking for plan execution.
//!
//! After a plan is approved, its implementation steps are converted into
//! `TodoItem`s that track progress (pending → in_progress → completed).
//!
//! Mirrors the Python todo handler pattern used in
//! `opendev/core/context_engineering/tools/handlers/todo_handler.py`.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::{debug, warn};

/// Status of a todo item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "todo"),
            Self::InProgress => write!(f, "doing"),
            Self::Completed => write!(f, "done"),
        }
    }
}

/// A single todo item derived from a plan step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// Unique ID within the todo list (1-based index).
    pub id: usize,
    /// Short title (the plan step text).
    pub title: String,
    /// Current status.
    pub status: TodoStatus,
    /// Present continuous text for spinner display (e.g., "Running tests").
    #[serde(default)]
    pub active_form: String,
    /// Completion notes / log entry.
    #[serde(default)]
    pub log: String,
    /// When the item was created.
    pub created_at: String,
    /// When the status last changed.
    pub updated_at: String,
}

/// Manager for tracking todos during plan execution.
///
/// Holds an ordered map of todo items and provides CRUD operations.
/// The manager is session-scoped (not persisted to disk by default).
#[derive(Debug, Clone, Default)]
pub struct TodoManager {
    todos: BTreeMap<usize, TodoItem>,
    next_id: usize,
}

impl TodoManager {
    /// Create a new, empty todo manager.
    pub fn new() -> Self {
        Self {
            todos: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Create a todo manager pre-populated from plan step strings.
    pub fn from_steps(steps: &[String]) -> Self {
        let mut mgr = Self::new();
        for step in steps {
            mgr.add(step.clone());
        }
        mgr
    }

    /// Add a new todo item. Returns its assigned ID.
    pub fn add(&mut self, title: String) -> usize {
        let now = Utc::now().to_rfc3339();
        let id = self.next_id;
        self.next_id += 1;
        self.todos.insert(
            id,
            TodoItem {
                id,
                title,
                status: TodoStatus::Pending,
                active_form: String::new(),
                log: String::new(),
                created_at: now.clone(),
                updated_at: now,
            },
        );
        debug!(id, "Added todo");
        id
    }

    /// Add a new todo item with initial status and active_form. Returns its assigned ID.
    pub fn add_with_status(
        &mut self,
        title: String,
        status: TodoStatus,
        active_form: String,
    ) -> usize {
        let now = Utc::now().to_rfc3339();
        let id = self.next_id;
        self.next_id += 1;
        // If adding as InProgress, enforce single-active constraint
        if status == TodoStatus::InProgress {
            self.revert_other_doing(id);
        }
        self.todos.insert(
            id,
            TodoItem {
                id,
                title,
                status,
                active_form,
                log: String::new(),
                created_at: now.clone(),
                updated_at: now,
            },
        );
        debug!(id, "Added todo with status");
        id
    }

    /// Replace the entire todo list with new items. Resets IDs starting from 1.
    pub fn write_todos(&mut self, items: Vec<(String, TodoStatus, String)>) {
        self.todos.clear();
        self.next_id = 1;
        for (title, status, active_form) in items {
            self.add_with_status(title, status, active_form);
        }
    }

    /// Update the status of a todo item by ID.
    ///
    /// Enforces single "doing" constraint: when setting InProgress,
    /// auto-reverts other "doing" items to Pending.
    ///
    /// Returns `true` if the item was found and updated.
    pub fn set_status(&mut self, id: usize, status: TodoStatus) -> bool {
        if !self.todos.contains_key(&id) {
            warn!(id, "Todo not found");
            return false;
        }
        // Enforce single-active constraint
        if status == TodoStatus::InProgress {
            self.revert_other_doing(id);
        }
        if let Some(item) = self.todos.get_mut(&id) {
            item.status = status;
            item.updated_at = Utc::now().to_rfc3339();
            debug!(id, %status, "Updated todo status");
        }
        true
    }

    /// Revert all "doing" items (except `except_id`) back to Pending.
    fn revert_other_doing(&mut self, except_id: usize) {
        let now = Utc::now().to_rfc3339();
        for item in self.todos.values_mut() {
            if item.id != except_id && item.status == TodoStatus::InProgress {
                item.status = TodoStatus::Pending;
                item.updated_at = now.clone();
                debug!(id = item.id, "Reverted doing→pending (single-active)");
            }
        }
    }

    /// Mark a todo as in-progress.
    pub fn start(&mut self, id: usize) -> bool {
        self.set_status(id, TodoStatus::InProgress)
    }

    /// Mark a todo as completed.
    pub fn complete(&mut self, id: usize) -> bool {
        self.set_status(id, TodoStatus::Completed)
    }

    /// Get a todo item by ID.
    pub fn get(&self, id: usize) -> Option<&TodoItem> {
        self.todos.get(&id)
    }

    /// Get all todo items in order.
    pub fn all(&self) -> Vec<&TodoItem> {
        self.todos.values().collect()
    }

    /// Get mutable access to the internal map (for title updates etc.).
    pub fn todos_mut(&mut self) -> &mut BTreeMap<usize, TodoItem> {
        &mut self.todos
    }

    /// Check if there are any todos.
    pub fn has_todos(&self) -> bool {
        !self.todos.is_empty()
    }

    /// Total number of todos.
    pub fn total(&self) -> usize {
        self.todos.len()
    }

    /// Number of completed todos.
    pub fn completed_count(&self) -> usize {
        self.todos
            .values()
            .filter(|t| t.status == TodoStatus::Completed)
            .count()
    }

    /// Number of in-progress todos.
    pub fn in_progress_count(&self) -> usize {
        self.todos
            .values()
            .filter(|t| t.status == TodoStatus::InProgress)
            .count()
    }

    /// Number of pending todos.
    pub fn pending_count(&self) -> usize {
        self.todos
            .values()
            .filter(|t| t.status == TodoStatus::Pending)
            .count()
    }

    /// Get the next pending todo (lowest ID).
    pub fn next_pending(&self) -> Option<&TodoItem> {
        self.todos
            .values()
            .find(|t| t.status == TodoStatus::Pending)
    }

    /// Whether all todos are completed.
    pub fn all_completed(&self) -> bool {
        !self.todos.is_empty()
            && self
                .todos
                .values()
                .all(|t| t.status == TodoStatus::Completed)
    }

    /// Format a status display string suitable for inclusion in prompts.
    ///
    /// Example output:
    /// ```text
    /// Todos (2/5 done):
    ///   [done] 1. Set up project structure
    ///   [done] 2. Add config parser
    ///   [doing] 3. Implement core logic
    ///   [todo] 4. Write tests
    ///   [todo] 5. Update docs
    /// ```
    pub fn format_status(&self) -> String {
        if self.todos.is_empty() {
            return "No todos.".to_string();
        }

        let done = self.completed_count();
        let total = self.total();
        let mut out = format!("Todos ({done}/{total} done):\n");

        for item in self.todos.values() {
            out.push_str(&format!(
                "  [{}] {}. {}\n",
                item.status, item.id, item.title
            ));
        }

        out
    }

    /// Remove a todo by ID. Returns `true` if it existed.
    pub fn remove(&mut self, id: usize) -> bool {
        self.todos.remove(&id).is_some()
    }

    /// Clear all todos.
    pub fn clear(&mut self) {
        self.todos.clear();
        self.next_id = 1;
    }

    /// Fuzzy-find a todo by ID string.
    ///
    /// Supports formats: `"todo-3"`, `"3"`, `"todo_3"`, exact title match,
    /// or partial title match.
    pub fn find_todo(&self, id_str: &str) -> Option<(usize, &TodoItem)> {
        let id_str = id_str.trim();

        // Try "todo-N" format
        if let Some(n) = id_str.strip_prefix("todo-")
            && let Ok(id) = n.parse::<usize>()
            && let Some(item) = self.todos.get(&id)
        {
            return Some((id, item));
        }

        // Try "todo_N" format
        if let Some(n) = id_str.strip_prefix("todo_")
            && let Ok(id) = n.parse::<usize>()
            && let Some(item) = self.todos.get(&id)
        {
            return Some((id, item));
        }

        // Try plain numeric
        if let Ok(id) = id_str.parse::<usize>()
            && let Some(item) = self.todos.get(&id)
        {
            return Some((id, item));
        }

        // Try exact title match (case-insensitive)
        let lower = id_str.to_lowercase();
        for item in self.todos.values() {
            if item.title.to_lowercase() == lower {
                return Some((item.id, item));
            }
        }

        // Try partial title match
        for item in self.todos.values() {
            if item.title.to_lowercase().contains(&lower) {
                return Some((item.id, item));
            }
        }

        None
    }

    /// Get the `active_form` of the currently "doing" item, if any.
    pub fn get_active_todo_message(&self) -> Option<String> {
        self.todos
            .values()
            .find(|t| t.status == TodoStatus::InProgress)
            .and_then(|t| {
                if t.active_form.is_empty() {
                    None
                } else {
                    Some(t.active_form.clone())
                }
            })
    }

    /// Whether there are any non-completed todos.
    pub fn has_incomplete_todos(&self) -> bool {
        self.todos
            .values()
            .any(|t| t.status != TodoStatus::Completed)
    }

    /// Format the todo list sorted by status: doing → todo → done.
    pub fn format_status_sorted(&self) -> String {
        if self.todos.is_empty() {
            return "No todos.".to_string();
        }

        let done = self.completed_count();
        let total = self.total();
        let mut out = format!("Todos ({done}/{total} done):\n");

        let mut items: Vec<&TodoItem> = self.todos.values().collect();
        items.sort_by_key(|i| match i.status {
            TodoStatus::InProgress => 0,
            TodoStatus::Pending => 1,
            TodoStatus::Completed => 2,
        });

        for item in items {
            out.push_str(&format!(
                "  [{}] {}. {}\n",
                item.status, item.id, item.title
            ));
        }

        out
    }
}

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
mod tests {
    use super::*;

    #[test]
    fn test_todo_manager_basic() {
        let mut mgr = TodoManager::new();
        assert!(!mgr.has_todos());
        assert_eq!(mgr.total(), 0);

        let id1 = mgr.add("First step".to_string());
        let id2 = mgr.add("Second step".to_string());

        assert!(mgr.has_todos());
        assert_eq!(mgr.total(), 2);
        assert_eq!(mgr.pending_count(), 2);
        assert_eq!(mgr.completed_count(), 0);

        assert_eq!(mgr.get(id1).unwrap().title, "First step");
        assert_eq!(mgr.get(id1).unwrap().status, TodoStatus::Pending);

        mgr.start(id1);
        assert_eq!(mgr.get(id1).unwrap().status, TodoStatus::InProgress);
        assert_eq!(mgr.in_progress_count(), 1);

        mgr.complete(id1);
        assert_eq!(mgr.get(id1).unwrap().status, TodoStatus::Completed);
        assert_eq!(mgr.completed_count(), 1);

        assert!(!mgr.all_completed());
        mgr.complete(id2);
        assert!(mgr.all_completed());
    }

    #[test]
    fn test_todo_manager_from_steps() {
        let steps = vec![
            "Set up project".to_string(),
            "Write code".to_string(),
            "Test".to_string(),
        ];
        let mgr = TodoManager::from_steps(&steps);
        assert_eq!(mgr.total(), 3);
        let items = mgr.all();
        assert_eq!(items[0].title, "Set up project");
        assert_eq!(items[1].title, "Write code");
        assert_eq!(items[2].title, "Test");
    }

    #[test]
    fn test_next_pending() {
        let mut mgr = TodoManager::from_steps(&["A".into(), "B".into(), "C".into()]);
        assert_eq!(mgr.next_pending().unwrap().id, 1);

        mgr.complete(1);
        assert_eq!(mgr.next_pending().unwrap().id, 2);

        mgr.complete(2);
        mgr.complete(3);
        assert!(mgr.next_pending().is_none());
    }

    #[test]
    fn test_format_status() {
        let mut mgr = TodoManager::from_steps(&["Step A".into(), "Step B".into()]);
        mgr.complete(1);
        let status = mgr.format_status();
        assert!(status.contains("1/2 done"));
        assert!(status.contains("[done] 1. Step A"));
        assert!(status.contains("[todo] 2. Step B"));
    }

    #[test]
    fn test_remove_and_clear() {
        let mut mgr = TodoManager::from_steps(&["A".into(), "B".into()]);
        assert!(mgr.remove(1));
        assert_eq!(mgr.total(), 1);
        assert!(!mgr.remove(1)); // Already removed

        mgr.clear();
        assert_eq!(mgr.total(), 0);
        assert!(!mgr.has_todos());
    }

    #[test]
    fn test_set_status_nonexistent() {
        let mut mgr = TodoManager::new();
        assert!(!mgr.set_status(999, TodoStatus::Completed));
    }

    #[test]
    fn test_parse_plan_steps_basic() {
        let plan = "\
# My Plan

---BEGIN PLAN---

## Summary
Do some stuff.

## Implementation Steps

1. Set up the project structure
2. Add the config parser
3. Implement core logic
4. Write tests
5. Update documentation

## Verification

1. Run tests
2. Check lint

---END PLAN---
";
        let steps = parse_plan_steps(plan);
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0], "Set up the project structure");
        assert_eq!(steps[4], "Update documentation");
    }

    #[test]
    fn test_parse_plan_steps_with_bold() {
        let plan = "\
## Implementation Steps

1. **Set up** the project
2. **Add** config handling
";
        let steps = parse_plan_steps(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0], "Set up the project");
        assert_eq!(steps[1], "Add config handling");
    }

    #[test]
    fn test_parse_plan_steps_stops_at_next_section() {
        let plan = "\
## Steps

1. First step
2. Second step

## Verification

1. Run tests
";
        let steps = parse_plan_steps(plan);
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn test_parse_plan_steps_empty() {
        let plan = "# Plan\n\nNo steps section here.\n";
        let steps = parse_plan_steps(plan);
        assert!(steps.is_empty());
    }

    #[test]
    fn test_extract_numbered_step_formats() {
        assert_eq!(
            extract_numbered_step("1. Do something"),
            Some("Do something".into())
        );
        assert_eq!(
            extract_numbered_step("12. Multi digit"),
            Some("Multi digit".into())
        );
        assert_eq!(
            extract_numbered_step("1) Paren format"),
            Some("Paren format".into())
        );
        assert_eq!(extract_numbered_step("Not a step"), None);
        assert_eq!(extract_numbered_step(""), None);
        assert_eq!(extract_numbered_step("  "), None);
    }

    #[test]
    fn test_todo_status_display() {
        assert_eq!(TodoStatus::Pending.to_string(), "todo");
        assert_eq!(TodoStatus::InProgress.to_string(), "doing");
        assert_eq!(TodoStatus::Completed.to_string(), "done");
    }

    #[test]
    fn test_find_todo_formats() {
        let mgr = TodoManager::from_steps(&["Alpha".into(), "Beta".into(), "Gamma".into()]);

        // Numeric
        assert_eq!(mgr.find_todo("2").unwrap().0, 2);
        // todo-N
        assert_eq!(mgr.find_todo("todo-1").unwrap().0, 1);
        // todo_N
        assert_eq!(mgr.find_todo("todo_3").unwrap().0, 3);
        // Exact title
        assert_eq!(mgr.find_todo("Beta").unwrap().0, 2);
        // Partial title
        assert_eq!(mgr.find_todo("alph").unwrap().0, 1);
        // Not found
        assert!(mgr.find_todo("xyz").is_none());
    }

    #[test]
    fn test_single_active_constraint() {
        let mut mgr = TodoManager::from_steps(&["A".into(), "B".into(), "C".into()]);
        mgr.start(1);
        assert_eq!(mgr.get(1).unwrap().status, TodoStatus::InProgress);

        // Starting another should revert the first
        mgr.start(2);
        assert_eq!(mgr.get(1).unwrap().status, TodoStatus::Pending);
        assert_eq!(mgr.get(2).unwrap().status, TodoStatus::InProgress);
    }

    #[test]
    fn test_add_with_status() {
        let mut mgr = TodoManager::new();
        let id = mgr.add_with_status(
            "Test item".into(),
            TodoStatus::InProgress,
            "Testing...".into(),
        );
        assert_eq!(mgr.get(id).unwrap().status, TodoStatus::InProgress);
        assert_eq!(mgr.get(id).unwrap().active_form, "Testing...");
    }

    #[test]
    fn test_write_todos() {
        let mut mgr = TodoManager::from_steps(&["Old".into()]);
        mgr.write_todos(vec![
            ("New A".into(), TodoStatus::Pending, String::new()),
            (
                "New B".into(),
                TodoStatus::InProgress,
                "Working on B".into(),
            ),
        ]);
        assert_eq!(mgr.total(), 2);
        assert_eq!(mgr.get(1).unwrap().title, "New A");
        assert_eq!(mgr.get(2).unwrap().active_form, "Working on B");
    }

    #[test]
    fn test_get_active_todo_message() {
        let mut mgr = TodoManager::new();
        mgr.add_with_status("Task".into(), TodoStatus::InProgress, "Doing task".into());
        assert_eq!(
            mgr.get_active_todo_message(),
            Some("Doing task".to_string())
        );
    }

    #[test]
    fn test_has_incomplete_todos() {
        let mut mgr = TodoManager::from_steps(&["A".into()]);
        assert!(mgr.has_incomplete_todos());
        mgr.complete(1);
        assert!(!mgr.has_incomplete_todos());
    }

    #[test]
    fn test_format_status_sorted() {
        let mut mgr = TodoManager::from_steps(&["A".into(), "B".into(), "C".into()]);
        mgr.start(2);
        mgr.complete(3);
        let status = mgr.format_status_sorted();
        // "doing" should appear before "todo" and "done"
        let doing_pos = status.find("[doing]").unwrap();
        let todo_pos = status.find("[todo]").unwrap();
        let done_pos = status.find("[done]").unwrap();
        assert!(doing_pos < todo_pos);
        assert!(todo_pos < done_pos);
    }

    #[test]
    fn test_parse_status() {
        assert_eq!(parse_status("pending"), Some(TodoStatus::Pending));
        assert_eq!(parse_status("todo"), Some(TodoStatus::Pending));
        assert_eq!(parse_status("in_progress"), Some(TodoStatus::InProgress));
        assert_eq!(parse_status("doing"), Some(TodoStatus::InProgress));
        assert_eq!(parse_status("in-progress"), Some(TodoStatus::InProgress));
        assert_eq!(parse_status("completed"), Some(TodoStatus::Completed));
        assert_eq!(parse_status("done"), Some(TodoStatus::Completed));
        assert_eq!(parse_status("complete"), Some(TodoStatus::Completed));
        assert_eq!(parse_status("unknown"), None);
    }

    #[test]
    fn test_strip_markdown() {
        assert_eq!(strip_markdown("**bold** text"), "bold text");
        assert_eq!(strip_markdown("`code`"), "code");
        assert_eq!(strip_markdown("~~struck~~"), "struck");
    }
}
