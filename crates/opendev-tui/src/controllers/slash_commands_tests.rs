use super::*;

#[test]
fn test_find_matching_empty() {
    let results = find_matching_commands("");
    assert_eq!(results.len(), BUILTIN_COMMANDS.len());
}

#[test]
fn test_find_matching_prefix() {
    let results = find_matching_commands("he");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "help");
}

#[test]
fn test_find_matching_multiple() {
    let results = find_matching_commands("task");
    assert_eq!(results.len(), 2); // "tasks" and "task"
}

#[test]
fn test_is_command() {
    assert!(is_command("help"));
    assert!(is_command("exit"));
    assert!(!is_command("nonexistent"));
}

#[test]
fn test_builtin_count() {
    assert_eq!(BUILTIN_COMMANDS.len(), 23);
}
