use super::*;

#[test]
fn test_command_completer_basic() {
    let c = CommandCompleter::new(None);
    let results = c.complete("hel");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].insert_text, "/help");
    assert_eq!(results[0].kind, CompletionKind::Command);
}

#[test]
fn test_command_completer_empty_query() {
    let c = CommandCompleter::new(None);
    let results = c.complete("");
    // Should return all built-in commands
    assert_eq!(results.len(), BUILTIN_COMMANDS.len());
}

#[test]
fn test_command_completer_no_match() {
    let c = CommandCompleter::new(None);
    let results = c.complete("zzzzz");
    assert!(results.is_empty());
}

#[test]
fn test_command_completer_extra_commands() {
    let extra = vec![SlashCommand {
        name: "custom",
        description: "a custom command",
    }];
    let c = CommandCompleter::new(Some(&extra));
    let results = c.complete("cust");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].insert_text, "/custom");
}

#[test]
fn test_command_completer_add_commands() {
    let mut c = CommandCompleter::new(None);
    let before = c.complete("").len();
    c.add_commands(&[SlashCommand {
        name: "newcmd",
        description: "new",
    }]);
    let after = c.complete("").len();
    assert_eq!(after, before + 1);
}

#[test]
fn test_symbol_completer_empty() {
    let c = SymbolCompleter::new();
    let results = c.complete("anything");
    assert!(results.is_empty());
}

#[test]
fn test_symbol_completer_with_symbols() {
    let mut c = SymbolCompleter::new();
    c.register_symbols(vec![
        ("MyStruct".to_string(), "struct".to_string()),
        ("my_function".to_string(), "fn".to_string()),
        ("MyEnum".to_string(), "enum".to_string()),
    ]);
    // "My" matches all three case-insensitively: MyStruct, my_function, MyEnum
    let results = c.complete("My");
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.kind == CompletionKind::Symbol));

    // "MyS" should only match MyStruct
    let results2 = c.complete("MyS");
    assert_eq!(results2.len(), 1);
    assert!(results2[0].label.contains("MyStruct"));
}

#[test]
fn test_file_completer_in_temp_dir() {
    let dir = tempfile::tempdir().unwrap();
    // Create a test file
    std::fs::write(dir.path().join("hello.txt"), "content").unwrap();
    let c = FileCompleter::new(dir.path().to_path_buf());
    let results = c.complete("hello");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].kind, CompletionKind::File);
    assert!(results[0].label.contains("hello.txt"));
}

// --- Argument completion tests ---

#[test]
fn test_arg_completion_mode() {
    let c = CommandCompleter::new(None);
    let results = c.complete_args("mode", "");
    assert_eq!(results.len(), 2);
    let labels: Vec<&str> = results.iter().map(|r| r.label.as_str()).collect();
    assert!(labels.contains(&"plan"));
    assert!(labels.contains(&"normal"));
}

#[test]
fn test_arg_completion_mode_prefix() {
    let c = CommandCompleter::new(None);
    let results = c.complete_args("mode", "pl");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "plan");
}

#[test]
fn test_arg_completion_autonomy() {
    let c = CommandCompleter::new(None);
    let results = c.complete_args("autonomy", "");
    assert_eq!(results.len(), 3);
}

#[test]
fn test_arg_completion_model_names() {
    let c = CommandCompleter::new(None);
    let results = c.complete_args("model", "claude");
    assert!(results.len() >= 2);
    for r in &results {
        assert!(r.label.starts_with("claude"));
    }
}

#[test]
fn test_arg_completion_mcp() {
    let c = CommandCompleter::new(None);
    let results = c.complete_args("mcp", "");
    assert_eq!(results.len(), 5);
}

#[test]
fn test_arg_completion_unknown_command() {
    let c = CommandCompleter::new(None);
    let results = c.complete_args("nonexistent", "");
    assert!(results.is_empty());
}
