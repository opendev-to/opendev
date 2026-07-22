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
fn test_adapter_for_provider_openai_chatgpt() {
    let adapter = AdaptedClient::adapter_for_provider("openai-chatgpt").unwrap();
    assert_eq!(adapter.provider_name(), "openai-chatgpt");
    assert_eq!(
        adapter.api_url(),
        "https://chatgpt.com/backend-api/codex/responses"
    );
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
fn headerless_chatgpt_responses_are_treated_as_sse() {
    assert!(is_sse_response("", true));
    assert!(is_sse_response(" ", true));
    assert!(is_sse_response("text/event-stream; charset=utf-8", false));
    assert!(!is_sse_response("", false));
    assert!(!is_sse_response("application/json", true));
}

#[test]
fn only_chatgpt_merges_streamed_output_into_a_sparse_final_response() {
    assert!(should_merge_streamed_chatgpt_output("openai-chatgpt"));
    assert!(!should_merge_streamed_chatgpt_output("openai"));
    assert!(!should_merge_streamed_chatgpt_output("anthropic"));
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

#[test]
fn curl_headers_are_written_to_a_private_config_file() {
    let config = curl_header_config(&["Authorization: Bearer not-in-argv".to_string()]).unwrap();
    let contents = std::fs::read_to_string(config.path()).unwrap();
    assert!(contents.contains("Authorization: Bearer not-in-argv"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(config.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn curl_transport_preserves_a_401_status_and_sends_auth_from_config() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 4096];
        let count = socket.read(&mut request).await.unwrap();
        assert!(
            String::from_utf8_lossy(&request[..count])
                .to_ascii_lowercase()
                .contains("authorization: bearer config-only-token")
        );
        let body = r#"{"error":{"message":"expired"}}"#;
        socket
            .write_all(
                format!(
                    "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });

    let client = AdaptedClient::new(
        HttpClient::new(format!("http://{address}"), HeaderMap::new(), None).unwrap(),
    )
    .with_curl_transport("Authorization: Bearer config-only-token".to_string());
    let result = client
        .post_json(&serde_json::json!({"ping": true}), None)
        .await
        .unwrap();
    assert_eq!(result.status, Some(401));
    assert!(!result.retryable);
}

#[tokio::test]
async fn curl_transport_preserves_redirects_for_dashscope_compatibility() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 4096];
        let _ = first.read(&mut request).await.unwrap();
        first
            .write_all(
                format!(
                    "HTTP/1.1 307 Temporary Redirect\r\nlocation: http://{address}/final\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();

        let (mut second, _) = listener.accept().await.unwrap();
        let count = second.read(&mut request).await.unwrap();
        assert!(String::from_utf8_lossy(&request[..count]).starts_with("POST /final"));
        let body = r#"{"ok":true}"#;
        second
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });

    let client = AdaptedClient::new(
        HttpClient::new(format!("http://{address}/start"), HeaderMap::new(), None).unwrap(),
    )
    .with_curl_transport("Authorization: Bearer redirect-token".to_string());
    let result = client
        .post_json(&serde_json::json!({"ping": true}), None)
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.body.unwrap()["ok"], true);
}

// --- Streaming error classification (issues #13, #110) ---
//
// `post_json_streaming` must preserve the retryable/fatal classification made
// by `send_streaming_request` instead of soft-failing everything as retryable:
// a bad API key (401) or wrong base URL (404) previously became an infinite
// silent retry loop in the react loop.

use crate::client::HttpClient;
use crate::models::RetryConfig;
use crate::streaming::FnStreamCallback;
use reqwest::header::HeaderMap;

/// Spawn a local HTTP server that answers every request with the given
/// status line and JSON body, then closes the connection. Returns the URL.
async fn spawn_static_server(status_line: &'static str, body: &'static str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                // Read the full request (headers + Content-Length body) so the
                // client never sees a broken pipe while writing.
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    let Ok(n) = sock.read(&mut chunk).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                        let content_length = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if buf.len() >= pos + 4 + content_length {
                            break;
                        }
                    }
                }
                let response = format!(
                    "{status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    format!("http://{addr}")
}

fn streaming_client_for(url: &str) -> AdaptedClient {
    let http = HttpClient::new(url, HeaderMap::new(), None)
        .unwrap()
        .with_retry_config(RetryConfig {
            max_retries: 0,
            initial_delay_ms: 1,
            ..RetryConfig::default()
        });
    AdaptedClient::with_adapter(http, AdaptedClient::adapter_for_provider("openai").unwrap())
}

#[tokio::test]
async fn test_streaming_401_fails_fast_not_retryable() {
    let url = spawn_static_server(
        "HTTP/1.1 401 Unauthorized",
        r#"{"error":{"message":"Invalid API key"}}"#,
    )
    .await;
    let client = streaming_client_for(&url);

    let cb = FnStreamCallback(|_| {});
    let result = client
        .post_json_streaming(
            &serde_json::json!({"model": "gpt-4o", "messages": []}),
            None,
            &cb,
        )
        .await
        .unwrap();

    assert!(!result.success);
    assert!(
        !result.retryable,
        "401 must be non-retryable, got: {:?}",
        result.error
    );
    let err = result.error.unwrap();
    assert!(err.contains("Invalid API key"), "error was: {err}");
    assert!(err.contains("request_id="), "error was: {err}");
}

#[tokio::test]
async fn test_streaming_404_fails_fast_not_retryable() {
    let url = spawn_static_server(
        "HTTP/1.1 404 Not Found",
        r#"{"error":{"message":"Unknown endpoint"}}"#,
    )
    .await;
    let client = streaming_client_for(&url);

    let cb = FnStreamCallback(|_| {});
    let result = client
        .post_json_streaming(
            &serde_json::json!({"model": "gpt-4o", "messages": []}),
            None,
            &cb,
        )
        .await
        .unwrap();

    assert!(!result.success);
    assert!(!result.retryable, "404 must be non-retryable");
}

#[tokio::test]
async fn test_streaming_503_stays_retryable_after_exhausted_retries() {
    let url = spawn_static_server(
        "HTTP/1.1 503 Service Unavailable",
        r#"{"error":{"message":"overloaded"}}"#,
    )
    .await;
    let client = streaming_client_for(&url);

    let cb = FnStreamCallback(|_| {});
    let result = client
        .post_json_streaming(
            &serde_json::json!({"model": "gpt-4o", "messages": []}),
            None,
            &cb,
        )
        .await
        .unwrap();

    assert!(!result.success);
    assert!(
        result.retryable,
        "transient 503 must stay retryable so the react loop can back off and retry"
    );
}

#[tokio::test]
async fn test_streaming_non_sse_body_is_reported_for_diagnosis() {
    let url = spawn_static_server("HTTP/1.1 200 OK", "upstream protocol mismatch").await;
    let client = streaming_client_for(&url);

    let cb = FnStreamCallback(|_| {});
    let error = client
        .post_json_streaming(
            &serde_json::json!({"model": "gpt-4o", "messages": []}),
            None,
            &cb,
        )
        .await
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("Unexpected non-SSE response"),
        "error was: {error}"
    );
    assert!(error.contains("application/json"), "error was: {error}");
    assert!(
        error.contains("upstream protocol mismatch"),
        "error was: {error}"
    );
}
