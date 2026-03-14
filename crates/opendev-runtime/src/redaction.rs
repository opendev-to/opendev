//! Secret redaction helpers for logs, sessions, and user-visible output.

use regex::Regex;

/// Redact common credential patterns from arbitrary text.
pub fn redact_secrets(text: &str) -> String {
    let patterns = [
        r"sk-ant-api03-[A-Za-z0-9_-]+",
        r"sk-[A-Za-z0-9_-]{16,}",
        r"gsk_[A-Za-z0-9_-]+",
        r"AIza[0-9A-Za-z_-]{20,}",
        r"gh[pousr]_[A-Za-z0-9_]+",
        r"Bearer\s+[A-Za-z0-9._-]{16,}",
        r#"(?i)(password\s*[:=]\s*["']?)[^"'\\s,]+"#,
        r"[A-Za-z0-9+/]{40,}={0,2}",
    ];

    let mut redacted = text.to_string();
    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            redacted = re.replace_all(&redacted, "[REDACTED]").into_owned();
        }
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::redact_secrets;

    #[test]
    fn redacts_anthropic_keys() {
        let text = "key sk-ant-api03-abcdefghij1234567890abcdefghij1234567890abcdefghij";
        let out = redact_secrets(text);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("abcdefghij1234567890"));
    }
}
