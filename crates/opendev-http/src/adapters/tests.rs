use super::*;

#[test]
fn test_detect_anthropic() {
    assert_eq!(
        detect_provider_from_key("sk-ant-api03-abc123"),
        Some("anthropic")
    );
}

#[test]
fn test_detect_openai() {
    assert_eq!(detect_provider_from_key("sk-proj-abc123"), Some("openai"));
    assert_eq!(detect_provider_from_key("sk-abc123"), Some("openai"));
}

#[test]
fn test_detect_groq() {
    assert_eq!(detect_provider_from_key("gsk_abc123def456"), Some("groq"));
}

#[test]
fn test_detect_gemini() {
    assert_eq!(detect_provider_from_key("AIzaSyAbc123"), Some("gemini"));
}

#[test]
fn test_detect_unknown() {
    assert_eq!(detect_provider_from_key("unknown-key-format"), None);
    assert_eq!(detect_provider_from_key(""), None);
}

#[test]
fn test_anthropic_before_openai() {
    // sk-ant- should match anthropic, not openai
    assert_eq!(
        detect_provider_from_key("sk-ant-api03-test"),
        Some("anthropic")
    );
}
