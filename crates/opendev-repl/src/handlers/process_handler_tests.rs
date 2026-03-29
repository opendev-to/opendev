use super::*;

fn handler() -> ProcessHandler {
    ProcessHandler::new(None)
}

#[test]
fn test_server_detection() {
    let h = handler();
    assert!(h.is_server_command("npm run dev"));
    assert!(h.is_server_command("flask run --port 5000"));
    assert!(h.is_server_command("uvicorn app:main"));
    assert!(h.is_server_command("cargo run"));
    assert!(!h.is_server_command("ls -la"));
    assert!(!h.is_server_command("git status"));
}

#[test]
fn test_pre_check_missing_command() {
    let h = handler();
    let args = HashMap::new();
    match h.pre_check("Bash", &args) {
        PreCheckResult::Deny(reason) => assert!(reason.contains("Missing")),
        other => panic!("Expected Deny, got {:?}", other),
    }
}

#[test]
fn test_pre_check_normal_command() {
    let h = handler();
    let mut args = HashMap::new();
    args.insert("command".to_string(), Value::String("ls -la".to_string()));
    match h.pre_check("Bash", &args) {
        PreCheckResult::Allow => {}
        other => panic!("Expected Allow, got {:?}", other),
    }
}

#[test]
fn test_pre_check_server_promoted_to_background() {
    let h = handler();
    let mut args = HashMap::new();
    args.insert(
        "command".to_string(),
        Value::String("npm run dev".to_string()),
    );
    match h.pre_check("Bash", &args) {
        PreCheckResult::ModifyArgs(new_args) => {
            assert_eq!(
                new_args.get("background").and_then(|v| v.as_bool()),
                Some(true)
            );
        }
        other => panic!("Expected ModifyArgs, got {:?}", other),
    }
}

#[test]
fn test_pre_check_already_background_no_modify() {
    let h = handler();
    let mut args = HashMap::new();
    args.insert(
        "command".to_string(),
        Value::String("npm run dev".to_string()),
    );
    args.insert("background".to_string(), Value::Bool(true));
    match h.pre_check("Bash", &args) {
        PreCheckResult::Allow => {}
        other => panic!("Expected Allow (already background), got {:?}", other),
    }
}

#[test]
fn test_truncate_output_short() {
    let output = "line1\nline2\nline3";
    assert_eq!(ProcessHandler::truncate_output(output), output);
}

#[test]
fn test_truncate_output_long() {
    let lines: Vec<String> = (0..500).map(|i| format!("line {i}")).collect();
    let output = lines.join("\n");
    let result = ProcessHandler::truncate_output(&output);
    assert!(result.contains("omitted"));
    assert!(result.lines().count() < 250);
}

#[test]
fn test_post_process_sets_meta() {
    let h = handler();
    let mut args = HashMap::new();
    args.insert("command".to_string(), Value::String("ls".to_string()));
    args.insert("background".to_string(), Value::Bool(true));

    let result = h.post_process("Bash", &args, Some("output"), None, true);
    assert!(result.meta.is_background);
    assert!(result.meta.operation_id.is_some());
}
