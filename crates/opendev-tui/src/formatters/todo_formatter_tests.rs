use super::*;

#[test]
fn test_write_todos_summary() {
    let output = "Todos (0/3 done):\n  [todo] 1. Set up\n  [todo] 2. Code\n  [todo] 3. Test\n";
    assert_eq!(
        summarize_todo_result("write_todos", output),
        "Created 3 todos"
    );
}

#[test]
fn test_list_todos_summary() {
    let output = "Todos (1/3 done):\n  [doing] 2. Code\n  [todo] 3. Test\n  [done] 1. Setup\n";
    assert_eq!(
        summarize_todo_result("list_todos", output),
        "3 todos (1 active, 1 done, 1 pending)"
    );
}

#[test]
fn test_clear_todos_summary() {
    assert_eq!(
        summarize_todo_result("clear_todos", "Cleared."),
        "All todos cleared"
    );
}

#[test]
fn test_handles() {
    let f = TodoFormatter;
    assert!(f.handles("write_todos"));
    assert!(f.handles("clear_todos"));
    assert!(!f.handles("read_file"));
}

#[test]
fn test_format_produces_empty_body() {
    let f = TodoFormatter;
    let result = f.format("write_todos", "[todo] 1. A\n[todo] 2. B\n");
    assert!(result.body.is_empty());
    assert!(result.footer.is_none());
}
