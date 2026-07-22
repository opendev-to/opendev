//! Mocked cross-module coverage for the disabled-by-default ChatGPT foundation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use opendev_http::chatgpt_auth::RequestAuthenticator;
use opendev_http::{AdaptedClient, ChatGptOAuthCredential, CredentialStore, HttpClient};
use reqwest::header::{HeaderMap, HeaderValue};
use tokio_util::sync::CancellationToken;

struct MockAuthenticator;
#[async_trait]
impl RequestAuthenticator for MockAuthenticator {
    async fn headers_for_request(
        &self,
        _: Option<&CancellationToken>,
    ) -> Result<HeaderMap, opendev_http::HttpError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer integration-token"),
        );
        headers.insert(
            "openai-beta",
            HeaderValue::from_static("responses=experimental"),
        );
        Ok(headers)
    }
    async fn force_refresh(
        &self,
        _: Option<&CancellationToken>,
    ) -> Result<(), opendev_http::HttpError> {
        Ok(())
    }
}

struct RefreshAuthenticator {
    refreshed: AtomicBool,
    refreshes: AtomicUsize,
}

#[async_trait]
impl RequestAuthenticator for RefreshAuthenticator {
    async fn headers_for_request(
        &self,
        _: Option<&CancellationToken>,
    ) -> Result<HeaderMap, opendev_http::HttpError> {
        let mut headers = HeaderMap::new();
        let token = if self.refreshed.load(Ordering::SeqCst) {
            "Bearer refreshed-token"
        } else {
            "Bearer stale-token"
        };
        headers.insert("authorization", HeaderValue::from_static(token));
        Ok(headers)
    }

    async fn force_refresh(
        &self,
        _: Option<&CancellationToken>,
    ) -> Result<(), opendev_http::HttpError> {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        self.refreshed.store(true, Ordering::SeqCst);
        Ok(())
    }
}
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[tokio::test]
async fn stored_credential_and_chatgpt_adapter_work_through_the_http_boundary() {
    let auth_dir = tempfile::tempdir().unwrap();
    let mut store = CredentialStore::new(Some(auth_dir.path().join("auth.json")));
    store
        .store_chatgpt_oauth(ChatGptOAuthCredential {
            access_token: "test-access-token".to_string(),
            refresh_token: "test-refresh-token".to_string(),
            expires_at_ms: 4_102_444_800_000,
            account_id: None,
        })
        .unwrap();
    assert!(store.get_chatgpt_oauth().is_some());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let request = Arc::new(Mutex::new(String::new()));
    let request_for_server = Arc::clone(&request);
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut raw = vec![0_u8; 8192];
        let count = socket.read(&mut raw).await.unwrap();
        *request_for_server.lock().await = String::from_utf8_lossy(&raw[..count]).into_owned();

        let body = serde_json::json!({
            "id": "resp_test",
            "model": "codex-test",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "hello"}]
            }]
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let http = HttpClient::new(format!("http://{address}"), Default::default(), None)
        .unwrap()
        .with_request_authenticator(Arc::new(MockAuthenticator));
    let adapter = AdaptedClient::adapter_for_provider("openai-chatgpt").unwrap();
    let client = AdaptedClient::with_adapter(http, adapter);
    let response = client
        .post_json(
            &serde_json::json!({
                "model": "gpt-5.4",
                "max_completion_tokens": 128000,
                "_reasoning_effort": "medium",
                "messages": [{"role": "user", "content": "hello"}]
            }),
            None,
        )
        .await
        .unwrap();

    assert!(response.success);
    assert_eq!(
        response.body.unwrap()["choices"][0]["message"]["content"],
        "hello"
    );
    let request = request.lock().await;
    assert!(request.contains("\"store\":false"));
    assert!(!request.contains("max_output_tokens"));
    assert!(request.contains("\"summary\":\"auto\""));
    assert!(request.contains("reasoning.encrypted_content"));
    assert!(request.contains("authorization: Bearer integration-token"));
    assert!(request.contains("openai-beta: responses=experimental"));
    assert!(!request.contains("test-access-token"));
}

#[tokio::test]
async fn a_401_forces_one_refresh_and_replays_with_fresh_headers() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_server = Arc::clone(&requests);
    tokio::spawn(async move {
        for (status, body) in [
            ("401 Unauthorized", r#"{"error":{"message":"expired"}}"#),
            ("200 OK", r#"{"ok":true}"#),
        ] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut raw = vec![0_u8; 8192];
            let count = socket.read(&mut raw).await.unwrap();
            requests_for_server
                .lock()
                .await
                .push(String::from_utf8_lossy(&raw[..count]).into_owned());
            socket
                .write_all(
                    format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let auth = Arc::new(RefreshAuthenticator {
        refreshed: AtomicBool::new(false),
        refreshes: AtomicUsize::new(0),
    });
    let http = HttpClient::new(format!("http://{address}"), HeaderMap::new(), None)
        .unwrap()
        .with_request_authenticator(auth.clone());
    let result = http
        .post_json(&serde_json::json!({"hello":"world"}), None)
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(auth.refreshes.load(Ordering::SeqCst), 1);
    let requests = requests.lock().await;
    assert!(requests[0].contains("authorization: Bearer stale-token"));
    assert!(requests[1].contains("authorization: Bearer refreshed-token"));
}

#[tokio::test]
async fn streaming_401_forces_one_refresh_and_replays_with_fresh_headers() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_server = Arc::clone(&requests);
    tokio::spawn(async move {
        for (status, content_type, body) in [
            (
                "401 Unauthorized",
                "application/json",
                r#"{"error":{"message":"expired"}}"#,
            ),
            ("200 OK", "text/event-stream", "data: [DONE]\n\n"),
        ] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut raw = vec![0_u8; 8192];
            let count = socket.read(&mut raw).await.unwrap();
            requests_for_server
                .lock()
                .await
                .push(String::from_utf8_lossy(&raw[..count]).into_owned());
            socket
                .write_all(
                    format!("HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let auth = Arc::new(RefreshAuthenticator {
        refreshed: AtomicBool::new(false),
        refreshes: AtomicUsize::new(0),
    });
    let http = HttpClient::new(format!("http://{address}"), HeaderMap::new(), None)
        .unwrap()
        .with_request_authenticator(auth.clone());
    let response = http
        .send_streaming_request(&format!("http://{address}"), &serde_json::json!({}), None)
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(auth.refreshes.load(Ordering::SeqCst), 1);
    let requests = requests.lock().await;
    assert!(requests[0].contains("authorization: Bearer stale-token"));
    assert!(requests[1].contains("authorization: Bearer refreshed-token"));
}
