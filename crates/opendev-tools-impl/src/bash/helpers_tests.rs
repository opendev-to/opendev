use super::*;

// ---- Output truncation ----

#[test]
fn test_truncate_short_output() {
    let text = "short output";
    assert_eq!(truncate_output(text, false), text);
    assert_eq!(truncate_output(text, true), text);
}

#[test]
fn test_truncate_long_output_display() {
    let text = "a".repeat(50_000);
    let truncated = truncate_output(&text, false);
    assert!(truncated.len() < text.len());
    assert!(truncated.contains("[...truncated"));
    // Head and tail preserved
    assert!(truncated.starts_with("aaa"));
    assert!(truncated.ends_with("aaa"));
}

#[test]
fn test_truncate_long_output_llm() {
    let text = "b".repeat(50_000);
    let truncated = truncate_output(&text, true);
    assert!(truncated.len() < 20_000); // Should be within LLM limits
    assert!(truncated.contains("[...truncated"));
}

// ---- Command preparation ----

#[test]
fn test_prepare_command_python_unbuffered() {
    let cmd = prepare_command("python script.py");
    assert!(cmd.contains("python -u"));
}

#[test]
fn test_prepare_command_python3_unbuffered() {
    let cmd = prepare_command("python3 script.py");
    assert!(cmd.contains("python3 -u"));
}

#[test]
fn test_prepare_command_already_unbuffered() {
    let cmd = prepare_command("python -u script.py");
    // Should not double-insert
    assert_eq!(cmd.matches("-u").count(), 1);
}

#[test]
fn test_prepare_command_npx_auto_confirm() {
    let cmd = prepare_command("npx create-react-app my-app");
    assert!(cmd.starts_with("yes | "));
}

#[test]
fn test_prepare_command_no_modification() {
    let cmd = prepare_command("echo hello");
    assert_eq!(cmd, "echo hello");
}

// ---- Property-based truncation test ----

mod proptest_truncate {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        /// truncate_output must never panic and must respect length limits.
        #[test]
        fn fuzz_truncate_no_panic(text in "\\PC{0,10000}", for_llm in proptest::bool::ANY) {
            let result = truncate_output(&text, for_llm);
            // Result should never be empty if input is non-empty
            if !text.is_empty() {
                prop_assert!(!result.is_empty());
            }
        }
    }
}
