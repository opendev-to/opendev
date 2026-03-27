use chrono::Utc;
use std::collections::BTreeMap;
use tracing::{debug, warn};

use super::{SubTodoItem, TodoItem, TodoStatus};

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
                children: Vec::new(),
            },
        );
        debug!(id, "Added todo");
        id
    }

    /// Add a new todo item with initial status, active_form, and children. Returns its assigned ID.
    pub fn add_with_status(
        &mut self,
        title: String,
        status: TodoStatus,
        active_form: String,
        children: Vec<SubTodoItem>,
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
                children,
            },
        );
        debug!(id, "Added todo with status");
        id
    }

    /// Replace the entire todo list with new items. Resets IDs starting from 1.
    pub fn write_todos(&mut self, items: Vec<(String, TodoStatus, String, Vec<SubTodoItem>)>) {
        self.todos.clear();
        self.next_id = 1;
        for (title, status, active_form, children) in items {
            self.add_with_status(title, status, active_form, children);
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
            for child in &item.children {
                out.push_str(&format!("      - {}\n", child.title));
            }
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

    /// Reset all "doing" (in-progress) todos back to "pending".
    ///
    /// Called when the agent loop exits (interrupt, error, timeout, or normal
    /// completion) to ensure no todos remain spinning in "doing" state.
    /// Returns the number of items reset.
    pub fn reset_stuck_todos(&mut self) -> usize {
        let now = Utc::now().to_rfc3339();
        let mut count = 0;
        for item in self.todos.values_mut() {
            if item.status == TodoStatus::InProgress {
                item.status = TodoStatus::Pending;
                item.updated_at = now.clone();
                debug!(id = item.id, title = %item.title, "Reset stuck 'doing' todo back to 'pending'");
                count += 1;
            }
        }
        count
    }

    /// Whether there are any non-completed todos.
    pub fn has_incomplete_todos(&self) -> bool {
        self.todos
            .values()
            .any(|t| t.status != TodoStatus::Completed)
    }

    /// Whether any todo has been started (moved beyond Pending).
    /// Returns true if at least one todo is InProgress or Completed.
    pub fn has_work_in_progress(&self) -> bool {
        self.todos
            .values()
            .any(|t| t.status != TodoStatus::Pending)
    }

    /// Format the todo list sorted by status: doing -> todo -> done.
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
            for child in &item.children {
                out.push_str(&format!("      - {}\n", child.title));
            }
        }

        out
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
            Vec::new(),
        );
        assert_eq!(mgr.get(id).unwrap().status, TodoStatus::InProgress);
        assert_eq!(mgr.get(id).unwrap().active_form, "Testing...");
    }

    #[test]
    fn test_write_todos() {
        let mut mgr = TodoManager::from_steps(&["Old".into()]);
        mgr.write_todos(vec![
            ("New A".into(), TodoStatus::Pending, String::new(), Vec::new()),
            (
                "New B".into(),
                TodoStatus::InProgress,
                "Working on B".into(),
                Vec::new(),
            ),
        ]);
        assert_eq!(mgr.total(), 2);
        assert_eq!(mgr.get(1).unwrap().title, "New A");
        assert_eq!(mgr.get(2).unwrap().active_form, "Working on B");
    }

    #[test]
    fn test_write_todos_with_children() {
        let mut mgr = TodoManager::new();
        mgr.write_todos(vec![
            (
                "Implement auth".into(),
                TodoStatus::Pending,
                "Implementing auth".into(),
                vec![
                    SubTodoItem { title: "Add login endpoint".into() },
                    SubTodoItem { title: "Add token validation".into() },
                ],
            ),
            (
                "Write tests".into(),
                TodoStatus::Pending,
                "Writing tests".into(),
                vec![
                    SubTodoItem { title: "Unit tests".into() },
                ],
            ),
        ]);
        // total() counts only parents
        assert_eq!(mgr.total(), 2);
        assert_eq!(mgr.get(1).unwrap().children.len(), 2);
        assert_eq!(mgr.get(1).unwrap().children[0].title, "Add login endpoint");
        assert_eq!(mgr.get(2).unwrap().children.len(), 1);

        // format_status includes children
        let status = mgr.format_status();
        assert!(status.contains("- Add login endpoint"));
        assert!(status.contains("- Add token validation"));
        assert!(status.contains("- Unit tests"));
    }

    #[test]
    fn test_get_active_todo_message() {
        let mut mgr = TodoManager::new();
        mgr.add_with_status("Task".into(), TodoStatus::InProgress, "Doing task".into(), Vec::new());
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
    fn test_has_work_in_progress() {
        let mut mgr = TodoManager::from_steps(&["A".into(), "B".into()]);
        // All pending — no work started
        assert!(!mgr.has_work_in_progress());

        // Start one — work in progress
        mgr.start(1);
        assert!(mgr.has_work_in_progress());

        // Complete one, other still pending — still has work in progress
        mgr.complete(1);
        assert!(mgr.has_work_in_progress());

        // Complete all — still true (Completed != Pending)
        mgr.complete(2);
        assert!(mgr.has_work_in_progress());
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
    fn test_reset_stuck_todos() {
        let mut mgr = TodoManager::from_steps(&["A".into(), "B".into(), "C".into()]);
        mgr.start(1);
        mgr.complete(3);

        // A is "doing", B is "pending", C is "done"
        assert_eq!(mgr.get(1).unwrap().status, TodoStatus::InProgress);
        assert_eq!(mgr.get(2).unwrap().status, TodoStatus::Pending);
        assert_eq!(mgr.get(3).unwrap().status, TodoStatus::Completed);

        let reset_count = mgr.reset_stuck_todos();
        assert_eq!(reset_count, 1);
        // A should be reset to pending
        assert_eq!(mgr.get(1).unwrap().status, TodoStatus::Pending);
        // B stays pending
        assert_eq!(mgr.get(2).unwrap().status, TodoStatus::Pending);
        // C stays done
        assert_eq!(mgr.get(3).unwrap().status, TodoStatus::Completed);
    }

    #[test]
    fn test_reset_stuck_todos_none_doing() {
        let mut mgr = TodoManager::from_steps(&["A".into(), "B".into()]);
        mgr.complete(1);
        let reset_count = mgr.reset_stuck_todos();
        assert_eq!(reset_count, 0);
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
}
