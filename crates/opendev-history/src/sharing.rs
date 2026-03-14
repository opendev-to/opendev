//! Session sharing: anonymize and publish a session transcript.
//!
//! [`share_session`] strips sensitive data (API keys, absolute file paths)
//! from a session and either posts it to a remote endpoint or saves it
//! as a local HTML file.

use opendev_models::Session;
use regex::Regex;
use tracing::{debug, info};

/// Patterns that look like API keys or tokens.
const SENSITIVE_PATTERNS: &[&str] = &[
    // OpenAI / Anthropic style keys
    r"sk-[A-Za-z0-9_-]{20,}",
    r"sk-ant-[A-Za-z0-9_-]{20,}",
    // Generic bearer tokens in text
    r"Bearer [A-Za-z0-9_.\-/+=]{20,}",
    // AWS-style keys
    r"AKIA[A-Z0-9]{16}",
];

/// Anonymize a session by redacting sensitive content and replacing
/// absolute file paths with relative ones.
pub fn anonymize_session(session: &Session) -> Session {
    let mut anon = session.clone();

    // Build combined regex for sensitive patterns.
    let combined = SENSITIVE_PATTERNS.join("|");
    let re = Regex::new(&combined).expect("compiled sensitive-pattern regex");

    // Redact messages.
    for msg in &mut anon.messages {
        msg.content = re.replace_all(&msg.content, "[REDACTED]").to_string();
        msg.content = redact_absolute_paths(&msg.content);

        // Redact tool call parameters and results.
        for tc in &mut msg.tool_calls {
            for value in tc.parameters.values_mut() {
                redact_json_value(value, &re);
            }
            if let Some(ref mut result) = tc.result {
                redact_json_value(result, &re);
            }
        }
    }

    // Remove metadata that might contain sensitive info.
    anon.metadata.remove("api_key");
    anon.metadata.remove("token");

    // Clear working directory (absolute path).
    anon.working_directory = None;
    anon.context_files.clear();

    anon
}

/// Redact absolute paths (Unix and Windows) in a string.
fn redact_absolute_paths(text: &str) -> String {
    let re = Regex::new(r"(/[a-zA-Z][a-zA-Z0-9_.\-/]*){2,}|[A-Z]:\\[^\s]+").unwrap();
    re.replace_all(text, "[PATH]").to_string()
}

/// Recursively redact sensitive patterns in a JSON value.
fn redact_json_value(value: &mut serde_json::Value, re: &Regex) {
    match value {
        serde_json::Value::String(s) => {
            *s = re.replace_all(s, "[REDACTED]").to_string();
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                redact_json_value(item, re);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                redact_json_value(v, re);
            }
        }
        _ => {}
    }
}

/// Generate a self-contained HTML page from a session transcript.
fn render_session_html(session: &Session) -> String {
    let title = session
        .metadata
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Shared Session");

    let mut body = String::new();
    for msg in &session.messages {
        let role_class = match msg.role {
            opendev_models::Role::User => "user",
            opendev_models::Role::Assistant => "assistant",
            opendev_models::Role::System => "system",
        };
        body.push_str(&format!(
            "<div class=\"message {role_class}\"><strong>{role_class}</strong><pre>{}</pre></div>\n",
            html_escape(&msg.content),
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
body {{ font-family: system-ui, sans-serif; max-width: 800px; margin: 2rem auto; padding: 0 1rem; }}
.message {{ margin: 1rem 0; padding: 1rem; border-radius: 8px; }}
.user {{ background: #e8f0fe; }}
.assistant {{ background: #f0f0f0; }}
.system {{ background: #fff3cd; }}
pre {{ white-space: pre-wrap; word-break: break-word; margin: 0; }}
</style>
</head>
<body>
<h1>{title}</h1>
<p>Session ID: {}</p>
<p>Created: {}</p>
{body}
</body>
</html>"#,
        session.id,
        session.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Share a session transcript.
///
/// If `endpoint` is a non-empty URL, the anonymized session is POSTed
/// as JSON.  The endpoint is expected to return a JSON body with a
/// `"url"` field containing the public sharing URL.
///
/// If `endpoint` is empty or not provided, the session is saved as a
/// local HTML file in the system temp directory, and the file path is
/// returned as the "URL".
pub async fn share_session(session: &Session, endpoint: &str) -> Result<String, String> {
    let anonymized = anonymize_session(session);

    if endpoint.is_empty() {
        // Save to local HTML file.
        let filename = format!("opendev-session-{}.html", anonymized.id);
        let path = std::env::temp_dir().join(&filename);

        let html = render_session_html(&anonymized);
        std::fs::write(&path, html).map_err(|e| format!("Failed to write HTML file: {}", e))?;

        let url = format!("file://{}", path.display());
        info!(path = %path.display(), "Session shared as local HTML");
        Ok(url)
    } else {
        // POST to remote endpoint.
        let client = reqwest::Client::new();
        let json_body = serde_json::to_value(&anonymized)
            .map_err(|e| format!("Failed to serialize session: {}", e))?;

        debug!(endpoint, "Posting anonymized session");

        let response = client
            .post(endpoint)
            .json(&json_body)
            .send()
            .await
            .map_err(|e| format!("Failed to POST session: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Share endpoint returned {}: {}", status, body));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse share response: {}", e))?;

        let url = result
            .get("url")
            .and_then(|u| u.as_str())
            .ok_or_else(|| "Share response missing 'url' field".to_string())?;

        info!(url, "Session shared successfully");
        Ok(url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use opendev_models::{ChatMessage, Role};
    use std::collections::HashMap;
    use std::path::Path;

    fn make_msg(role: Role, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
            tool_calls: vec![],
            tokens: None,
            thinking_trace: None,
            reasoning_content: None,
            token_usage: None,
            provenance: None,
        }
    }

    #[test]
    fn test_anonymize_redacts_api_keys() {
        let mut session = Session::new();
        session.messages.push(make_msg(
            Role::User,
            "My key is sk-abcdefghijklmnopqrstuvwxyz123456 please use it",
        ));

        let anon = anonymize_session(&session);
        assert!(!anon.messages[0].content.contains("sk-"));
        assert!(anon.messages[0].content.contains("[REDACTED]"));
    }

    #[test]
    fn test_anonymize_redacts_absolute_paths() {
        let mut session = Session::new();
        session.messages.push(make_msg(
            Role::User,
            "The file is at /Users/john/codes/project/main.rs",
        ));

        let anon = anonymize_session(&session);
        assert!(!anon.messages[0].content.contains("/Users/john"));
        assert!(anon.messages[0].content.contains("[PATH]"));
    }

    #[test]
    fn test_anonymize_clears_working_directory() {
        let mut session = Session::new();
        session.working_directory = Some("/Users/john/codes/project".to_string());
        session.context_files = vec!["src/main.rs".to_string()];

        let anon = anonymize_session(&session);
        assert!(anon.working_directory.is_none());
        assert!(anon.context_files.is_empty());
    }

    #[test]
    fn test_anonymize_redacts_bearer_tokens() {
        let mut session = Session::new();
        session.messages.push(make_msg(
            Role::Assistant,
            "Using Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature for auth",
        ));

        let anon = anonymize_session(&session);
        assert!(!anon.messages[0].content.contains("eyJhbGci"));
        assert!(anon.messages[0].content.contains("[REDACTED]"));
    }

    #[test]
    fn test_anonymize_preserves_regular_content() {
        let mut session = Session::new();
        session
            .messages
            .push(make_msg(Role::User, "Hello, how are you?"));

        let anon = anonymize_session(&session);
        assert_eq!(anon.messages[0].content, "Hello, how are you?");
    }

    #[test]
    fn test_render_session_html() {
        let mut session = Session::new();
        session
            .metadata
            .insert("title".into(), serde_json::json!("Test Session"));
        session.messages.push(make_msg(Role::User, "Hello <world>"));
        session
            .messages
            .push(make_msg(Role::Assistant, "Hi & welcome"));

        let html = render_session_html(&session);
        assert!(html.contains("<title>Test Session</title>"));
        assert!(html.contains("Hello &lt;world&gt;"));
        assert!(html.contains("Hi &amp; welcome"));
        assert!(html.contains("user"));
        assert!(html.contains("assistant"));
    }

    #[tokio::test]
    async fn test_share_session_local_html() {
        let mut session = Session::new();
        session.id = "share-test-local".to_string();
        session.messages.push(make_msg(Role::User, "hello"));

        let result = share_session(&session, "").await;
        assert!(result.is_ok());
        let url = result.unwrap();
        assert!(url.starts_with("file://"));
        assert!(url.contains("share-test-local"));

        // Verify the file exists.
        let path = url.trim_start_matches("file://");
        assert!(Path::new(path).exists());
        std::fs::remove_file(path).ok();
    }

    #[tokio::test]
    async fn test_share_session_remote_failure() {
        let mut session = Session::new();
        session.messages.push(make_msg(Role::User, "hello"));

        let result = share_session(&session, "http://127.0.0.1:1/nonexistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape("\"quote\""), "&quot;quote&quot;");
    }

    #[test]
    fn test_redact_json_value() {
        let combined = SENSITIVE_PATTERNS.join("|");
        let re = Regex::new(&combined).unwrap();
        let mut val = serde_json::json!({
            "key": "sk-abcdefghijklmnopqrstuvwxyz123456",
            "nested": {
                "token": "Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.payload.sig"
            },
            "list": ["normal", "sk-aaaaaabbbbbbccccccdddddd"]
        });
        redact_json_value(&mut val, &re);
        assert!(val["key"].as_str().unwrap().contains("[REDACTED]"));
        assert!(
            val["nested"]["token"]
                .as_str()
                .unwrap()
                .contains("[REDACTED]")
        );
        assert_eq!(val["list"][0].as_str().unwrap(), "normal");
        assert!(val["list"][1].as_str().unwrap().contains("[REDACTED]"));
    }
}
