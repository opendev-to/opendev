use super::*;

#[test]
fn test_strip_system_reminders() {
    let input = "Hello\n<system-reminder>secret stuff</system-reminder>\nWorld";
    let result = strip_system_reminders(input);
    assert_eq!(result, "Hello\nWorld");
}

#[test]
fn test_strip_system_reminders_multiple() {
    let input = "A<system-reminder>x</system-reminder>B<system-reminder>y</system-reminder>C";
    let result = strip_system_reminders(input);
    assert_eq!(result, "ABC");
}

#[test]
fn test_strip_system_reminders_none() {
    let input = "No reminders here";
    let result = strip_system_reminders(input);
    assert_eq!(result, "No reminders here");
}

#[test]
fn test_truncate_output_short() {
    let text = "line1\nline2\nline3";
    let (result, truncated, _) = truncate_output(text, 5, 5);
    assert!(!truncated);
    assert_eq!(result, text);
}

#[test]
fn test_truncate_output_long() {
    let lines: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
    let text = lines.join("\n");
    let (result, truncated, hidden) = truncate_output(&text, 3, 3);
    assert!(truncated);
    assert_eq!(hidden, 14);
    assert!(result.contains("line 0"));
    assert!(result.contains("line 19"));
    assert!(result.contains("14 lines hidden"));
}
