use super::*;

#[test]
fn test_is_reasoning_model() {
    assert!(is_reasoning_model("o1-preview"));
    assert!(is_reasoning_model("o1-mini"));
    assert!(is_reasoning_model("o3-mini"));
    assert!(is_reasoning_model("o4-mini"));
    assert!(is_reasoning_model("codex-mini"));
    assert!(is_reasoning_model("gpt-5-turbo"));
    assert!(is_reasoning_model("gpt-5.4-mini"));
    assert!(!is_reasoning_model("gpt-4o"));
    assert!(!is_reasoning_model("claude-3-opus"));
}

#[test]
fn test_uses_max_completion_tokens() {
    assert!(uses_max_completion_tokens("o1-preview"));
    assert!(uses_max_completion_tokens("o3-mini"));
    assert!(uses_max_completion_tokens("o4-mini"));
    assert!(uses_max_completion_tokens("gpt-5-turbo"));
    assert!(!uses_max_completion_tokens("gpt-4o"));
    assert!(!uses_max_completion_tokens("claude-3-opus"));
    assert!(!uses_max_completion_tokens("codex-mini")); // codex uses max_completion_tokens? No — not in prefix list
}

#[test]
fn test_supports_temperature() {
    assert!(supports_temperature("gpt-4o"));
    assert!(supports_temperature("claude-3-opus"));
    assert!(!supports_temperature("o1-preview"));
    assert!(!supports_temperature("o3-mini"));
    assert!(!supports_temperature("codex-mini"));
    assert!(!supports_temperature("gpt-5-turbo"));
    assert!(!supports_temperature("gpt-5.4-mini"));
}

#[test]
fn test_case_insensitive_model_detection() {
    assert!(is_reasoning_model("O1-Preview"));
    assert!(is_reasoning_model("O3-MINI"));
    assert!(uses_max_completion_tokens("GPT-5-turbo"));
}
