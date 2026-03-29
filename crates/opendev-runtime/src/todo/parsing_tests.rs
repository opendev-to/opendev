use super::*;

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
