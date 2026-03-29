use super::*;

#[test]
fn test_detect_anthropic_key() {
    let text = "key=sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890";
    let matches = detect_secrets(text);
    assert!(!matches.is_empty());
    assert!(
        matches
            .iter()
            .any(|m| m.kind == SecretKind::AnthropicApiKey)
    );
}

#[test]
fn test_detect_openai_key() {
    let text = "OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz";
    let matches = detect_secrets(text);
    assert!(matches.iter().any(|m| m.kind == SecretKind::OpenAiApiKey));
}

#[test]
fn test_detect_groq_key() {
    let text = "export GROQ_KEY=gsk_abcdefghijklmnopqrstuvwxyz1234";
    let matches = detect_secrets(text);
    assert!(matches.iter().any(|m| m.kind == SecretKind::GroqApiKey));
}

#[test]
fn test_detect_google_key() {
    let text = "api_key: AIzaSyAbcdefghijklmnopqrstuvwxyz0123456789";
    let matches = detect_secrets(text);
    assert!(matches.iter().any(|m| m.kind == SecretKind::GoogleApiKey));
}

#[test]
fn test_detect_github_token() {
    let text = "Authorization: token ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh";
    let matches = detect_secrets(text);
    assert!(matches.iter().any(|m| m.kind == SecretKind::GitHubToken));
}

#[test]
fn test_detect_bearer_token() {
    let text = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload";
    let matches = detect_secrets(text);
    assert!(matches.iter().any(|m| m.kind == SecretKind::BearerToken));
}

#[test]
fn test_detect_password_assignment() {
    let text = "password=mysupersecretpassword123";
    let matches = detect_secrets(text);
    assert!(
        matches
            .iter()
            .any(|m| m.kind == SecretKind::PasswordAssignment)
    );
}

#[test]
fn test_detect_password_case_insensitive() {
    let text = "PASSWORD = hunter2";
    let matches = detect_secrets(text);
    assert!(
        matches
            .iter()
            .any(|m| m.kind == SecretKind::PasswordAssignment)
    );
}

#[test]
fn test_detect_base64_blob() {
    let text = "data: ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/==";
    let matches = detect_secrets(text);
    assert!(matches.iter().any(|m| m.kind == SecretKind::Base64Blob));
}

#[test]
fn test_no_false_positive_short_string() {
    let text = "hello world this is normal text";
    let matches = detect_secrets(text);
    assert!(matches.is_empty());
}

#[test]
fn test_redact_secrets_single() {
    let text = "key=sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890 done";
    let redacted = redact_secrets(text);
    assert!(redacted.contains("[REDACTED]"));
    assert!(!redacted.contains("sk-ant-"));
    assert!(redacted.contains("done"));
}

#[test]
fn test_redact_secrets_multiple() {
    let text = "key1=sk-ant-api03-abcdefghijklmnopqrstuvwxyz12345 key2=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh";
    let redacted = redact_secrets(text);
    assert!(!redacted.contains("sk-ant-"));
    assert!(!redacted.contains("ghp_"));
    // Should have two [REDACTED] markers
    assert_eq!(redacted.matches("[REDACTED]").count(), 2);
}

#[test]
fn test_redact_no_secrets() {
    let text = "just normal output\nno secrets here";
    assert_eq!(redact_secrets(text), text);
}

#[test]
fn test_redact_preserves_surrounding_text() {
    let text = "before password=secret123 after";
    let redacted = redact_secrets(text);
    assert!(redacted.starts_with("before "));
    assert!(redacted.ends_with(" after"));
    assert!(redacted.contains("[REDACTED]"));
}

#[test]
fn test_detect_passwd_variant() {
    let text = "passwd=supersecret";
    let matches = detect_secrets(text);
    assert!(
        matches
            .iter()
            .any(|m| m.kind == SecretKind::PasswordAssignment)
    );
}

#[test]
fn test_secret_match_positions() {
    let text = "prefix sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890 suffix";
    let matches = detect_secrets(text);
    assert!(!matches.is_empty());
    let m = &matches[0];
    assert_eq!(&text[m.start..m.end], m.matched_text);
}
