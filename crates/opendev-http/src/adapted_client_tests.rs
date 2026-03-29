use super::*;

#[test]
fn test_adapter_for_provider_anthropic() {
    let adapter = AdaptedClient::adapter_for_provider("anthropic").unwrap();
    assert_eq!(adapter.provider_name(), "anthropic");
}

#[test]
fn test_adapter_for_provider_openai() {
    let adapter = AdaptedClient::adapter_for_provider("openai").unwrap();
    assert_eq!(adapter.provider_name(), "openai");
}

#[test]
fn test_adapter_for_provider_gemini() {
    let adapter = AdaptedClient::adapter_for_provider("gemini").unwrap();
    assert_eq!(adapter.provider_name(), "gemini");
}

#[test]
fn test_adapter_for_provider_google() {
    let adapter = AdaptedClient::adapter_for_provider("google").unwrap();
    assert_eq!(adapter.provider_name(), "gemini");
}

#[test]
fn test_adapter_for_provider_groq_is_none() {
    assert!(AdaptedClient::adapter_for_provider("groq").is_none());
}

#[test]
fn test_adapter_for_provider_unknown_is_none() {
    assert!(AdaptedClient::adapter_for_provider("custom").is_none());
}

#[test]
fn test_resolve_provider_explicit() {
    assert_eq!(
        AdaptedClient::resolve_provider("anthropic", ""),
        "anthropic"
    );
    assert_eq!(
        AdaptedClient::resolve_provider("custom", "sk-ant-abc"),
        "custom"
    );
}

#[test]
fn test_resolve_provider_auto_detect() {
    assert_eq!(
        AdaptedClient::resolve_provider("", "sk-ant-api03-abc"),
        "anthropic"
    );
    assert_eq!(AdaptedClient::resolve_provider("", "sk-proj-abc"), "openai");
    assert_eq!(
        AdaptedClient::resolve_provider("", "AIzaSyAbc123"),
        "gemini"
    );
    assert_eq!(AdaptedClient::resolve_provider("", "gsk_abc123"), "groq");
}

#[test]
fn test_resolve_provider_fallback_to_openai() {
    assert_eq!(AdaptedClient::resolve_provider("", "unknown-key"), "openai");
}
