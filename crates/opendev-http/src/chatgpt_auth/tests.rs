use super::authenticator::RequestAuthenticator;
use super::*;
use crate::{ChatGptOAuthCredential, CredentialStore, HttpClient};
use base64::Engine;
use std::sync::{Arc, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn browser_login_test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[tokio::test]
async fn missing_chatgpt_credential_directs_users_to_setup() {
    let temp = tempfile::tempdir().unwrap();
    let auth = ChatGptAuthenticator::with_token_url(
        CredentialStore::new(Some(temp.path().join("auth.json"))),
        "http://127.0.0.1:1/token".to_string(),
    );

    let error = auth
        .headers_for_request(None)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("opendev setup"));
    assert!(!error.contains("opendev auth"));
}

#[tokio::test]
async fn expiring_credential_refreshes_and_preserves_rotated_token_when_omitted() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("auth.json");
    let mut store = CredentialStore::new(Some(path.clone()));
    store
        .store_chatgpt_oauth(ChatGptOAuthCredential {
            access_token: "old-access".into(),
            refresh_token: "keep-refresh".into(),
            expires_at_ms: 0,
            account_id: None,
        })
        .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        let size = stream.read(&mut bytes).await.unwrap();
        let request = String::from_utf8_lossy(&bytes[..size]);
        assert!(request.contains("grant_type=refresh_token"));
        assert!(request.contains("refresh_token=keep-refresh"));
        let body = r#"{"access_token":"new-access","expires_in":3600}"#;
        stream.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
    });
    let auth = ChatGptAuthenticator::with_token_url(CredentialStore::new(Some(path)), url);
    let headers = auth.headers_for_request(None).await.unwrap();
    assert_eq!(headers["authorization"], "Bearer new-access");
    assert_eq!(headers["openai-beta"], "responses=experimental");
    let credential = CredentialStore::new(Some(temp.path().join("auth.json")))
        .get_chatgpt_oauth()
        .unwrap();
    assert_eq!(credential.refresh_token, "keep-refresh");
}

#[tokio::test]
async fn concurrent_requests_share_one_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("auth.json");
    CredentialStore::new(Some(path.clone()))
        .store_chatgpt_oauth(ChatGptOAuthCredential {
            access_token: "old-access".into(),
            refresh_token: "shared-refresh".into(),
            expires_at_ms: 0,
            account_id: None,
        })
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        let _ = stream.read(&mut bytes).await.unwrap();
        let body = r#"{"access_token":"fresh-access","expires_in":3600}"#;
        stream.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let auth = ChatGptAuthenticator::with_token_url(CredentialStore::new(Some(path)), url);
    let (first, second) = tokio::join!(
        auth.headers_for_request(None),
        auth.headers_for_request(None)
    );
    assert_eq!(first.unwrap()["authorization"], "Bearer fresh-access");
    assert_eq!(second.unwrap()["authorization"], "Bearer fresh-access");
}

#[tokio::test]
async fn invalid_grant_refresh_removes_the_unusable_local_credential() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("auth.json");
    CredentialStore::new(Some(path.clone()))
        .store_chatgpt_oauth(ChatGptOAuthCredential {
            access_token: "expired-access".into(),
            refresh_token: "revoked-refresh".into(),
            expires_at_ms: 0,
            account_id: None,
        })
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let token_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = r#"{"error":"invalid_grant"}"#;
        stream
            .write_all(
                format!("HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes(),
            )
            .await
            .unwrap();
    });
    let auth =
        ChatGptAuthenticator::with_token_url(CredentialStore::new(Some(path.clone())), token_url);
    let error = auth.headers_for_request(None).await.unwrap_err();
    assert!(error.to_string().contains("login expired"));
    assert!(
        CredentialStore::new(Some(path))
            .get_chatgpt_oauth()
            .is_none()
    );
}

#[tokio::test]
async fn browser_login_uses_loopback_pkce_callback_and_rejects_wrong_state() {
    let _lock = browser_login_test_lock().lock().await;
    let temp = tempfile::tempdir().unwrap();
    let auth = ChatGptAuthenticator::new(CredentialStore::new(Some(temp.path().join("auth.json"))))
        .unwrap();
    let login = auth.begin_browser_login().await.unwrap();
    assert!(
        login
            .authorization_url
            .contains("code_challenge_method=S256")
    );
    assert!(
        login
            .authorization_url
            .contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback")
    );
    let state = "wrong-state";
    let callback_host = if login.has_ipv6_listener_for_test() {
        "[::1]"
    } else {
        "127.0.0.1"
    };
    tokio::spawn(async move {
        let mut stream = tokio::net::TcpStream::connect(format!("{callback_host}:1455"))
            .await
            .unwrap();
        let request = format!(
            "GET /auth/callback?code=do-not-exchange&state={state} HTTP/1.1\r\nHost: localhost\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
    });
    let error = auth.finish_browser_login(login, None).await.unwrap_err();
    assert!(error.to_string().contains("state or code was invalid"));
}

#[tokio::test]
async fn browser_login_exchanges_a_valid_callback_code_and_reports_denial() {
    let _lock = browser_login_test_lock().lock().await;
    let temp = tempfile::tempdir().unwrap();
    let token_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let token_url = format!("http://{}/token", token_listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut stream, _) = token_listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let count = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..count]);
        assert!(request.contains("code=accepted-code"));
        let body = r#"{"access_token":"browser-access","refresh_token":"browser-refresh","expires_in":3600}"#;
        stream
            .write_all(
                format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes(),
            )
            .await
            .unwrap();
    });

    let path = temp.path().join("auth.json");
    let auth =
        ChatGptAuthenticator::with_token_url(CredentialStore::new(Some(path.clone())), token_url);
    let login = auth.begin_browser_login().await.unwrap();
    let state = login.state_for_test().to_string();
    let callback_host = if login.has_ipv6_listener_for_test() {
        "[::1]"
    } else {
        "127.0.0.1"
    };
    tokio::spawn(async move {
        let mut stream = tokio::net::TcpStream::connect(format!("{callback_host}:1455"))
            .await
            .unwrap();
        let request = format!(
            "GET /auth/callback?code=accepted-code&state={state} HTTP/1.1\r\nHost: localhost\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
    });
    auth.finish_browser_login(login, None).await.unwrap();
    assert_eq!(
        CredentialStore::new(Some(path))
            .get_chatgpt_oauth()
            .unwrap()
            .access_token,
        "browser-access"
    );

    let denied = auth.begin_browser_login().await.unwrap();
    let callback_host = if denied.has_ipv6_listener_for_test() {
        "[::1]"
    } else {
        "127.0.0.1"
    };
    tokio::spawn(async move {
        let mut stream = tokio::net::TcpStream::connect(format!("{callback_host}:1455"))
            .await
            .unwrap();
        stream
            .write_all(
                b"GET /auth/callback?error=access_denied HTTP/1.1\r\nHost: localhost\r\n\r\n",
            )
            .await
            .unwrap();
    });
    let error = auth.finish_browser_login(denied, None).await.unwrap_err();
    assert!(error.to_string().contains("authorization was denied"));
}

#[tokio::test]
async fn device_login_polls_pending_then_exchanges_returned_code() {
    let temp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        for (status, body) in [
            (
                "200 OK",
                r#"{"device_auth_id":"device-id","user_code":"ABCD-EFGH","interval":0}"#,
            ),
            ("403 Forbidden", r#"{}"#),
            (
                "200 OK",
                r#"{"authorization_code":"authorization-code","code_verifier":"device-verifier"}"#,
            ),
            (
                "200 OK",
                r#"{"access_token":"device-access","refresh_token":"device-refresh","expires_in":3600}"#,
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut raw = [0_u8; 4096];
            let _ = stream.read(&mut raw).await.unwrap();
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let path = temp.path().join("auth.json");
    let auth = ChatGptAuthenticator::with_endpoints(
        CredentialStore::new(Some(path.clone())),
        format!("{base}/oauth/token"),
        format!("{base}/device/code"),
        format!("{base}/device/poll"),
    );
    let login = auth.begin_device_login(None).await.unwrap();
    assert_eq!(login.user_code, "ABCD-EFGH");
    assert_eq!(login.verification_url, protocol::DEVICE_VERIFY_URL);
    auth.finish_device_login(login, None).await.unwrap();
    assert_eq!(
        CredentialStore::new(Some(path))
            .get_chatgpt_oauth()
            .unwrap()
            .access_token,
        "device-access"
    );
}

#[tokio::test]
async fn device_login_cancellation_is_immediate() {
    let temp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut raw = [0_u8; 4096];
        let _ = stream.read(&mut raw).await.unwrap();
        let body = r#"{"device_auth_id":"device-id","user_code":"ABCD-EFGH","interval":60}"#;
        stream.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let auth = ChatGptAuthenticator::with_endpoints(
        CredentialStore::new(Some(temp.path().join("auth.json"))),
        format!("{base}/oauth/token"),
        format!("{base}/device/code"),
        format!("{base}/device/poll"),
    );
    let login = auth.begin_device_login(None).await.unwrap();
    let cancelled = tokio_util::sync::CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        auth.finish_device_login(login, Some(&cancelled)).await,
        Err(crate::HttpError::Interrupted)
    ));
}

#[test]
fn account_id_is_derived_from_the_chatgpt_auth_claim() {
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acct_123"}}"#);
    let token = format!("header.{payload}.signature");
    assert_eq!(
        super::authenticator::account_claim(&token).as_deref(),
        Some("acct_123")
    );
}

#[tokio::test]
async fn mocked_device_login_refresh_inference_and_logout_end_to_end() {
    let temp = tempfile::tempdir().unwrap();
    let auth_path = temp.path().join("auth.json");

    let oauth_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let oauth_base = format!("http://{}", oauth_listener.local_addr().unwrap());
    tokio::spawn(async move {
        for body in [
            r#"{"device_auth_id":"device-id","user_code":"ABCD-EFGH","interval":0}"#,
            r#"{"authorization_code":"authorization-code","code_verifier":"device-verifier"}"#,
            r#"{"access_token":"initial-access","refresh_token":"initial-refresh","expires_in":3600}"#,
            r#"{"access_token":"refreshed-access","refresh_token":"rotated-refresh","expires_in":3600}"#,
        ] {
            let (mut stream, _) = oauth_listener.accept().await.unwrap();
            let mut raw = [0_u8; 4096];
            let _ = stream.read(&mut raw).await.unwrap();
            stream
                .write_all(
                    format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes(),
                )
                .await
                .unwrap();
        }
    });

    let inference_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let inference_url = format!("http://{}", inference_listener.local_addr().unwrap());
    tokio::spawn(async move {
        for (status, body) in [
            ("401 Unauthorized", r#"{"error":{"message":"expired"}}"#),
            ("200 OK", r#"{"ok":true}"#),
        ] {
            let (mut stream, _) = inference_listener.accept().await.unwrap();
            let mut raw = [0_u8; 4096];
            let count = stream.read(&mut raw).await.unwrap();
            let request = String::from_utf8_lossy(&raw[..count]);
            if status == "401 Unauthorized" {
                assert!(request.contains("authorization: Bearer initial-access"));
            } else {
                assert!(request.contains("authorization: Bearer refreshed-access"));
            }
            stream
                .write_all(
                    format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes(),
                )
                .await
                .unwrap();
        }
    });

    let authenticator = Arc::new(ChatGptAuthenticator::with_endpoints(
        CredentialStore::new(Some(auth_path)),
        format!("{oauth_base}/oauth/token"),
        format!("{oauth_base}/device/code"),
        format!("{oauth_base}/device/poll"),
    ));
    let device = authenticator.begin_device_login(None).await.unwrap();
    authenticator
        .finish_device_login(device, None)
        .await
        .unwrap();
    assert!(matches!(authenticator.status(), LoginStatus::Active { .. }));

    let client = HttpClient::new(inference_url, Default::default(), None)
        .unwrap()
        .with_request_authenticator(authenticator.clone());
    assert!(
        client
            .post_json(&serde_json::json!({"model":"codex-test"}), None)
            .await
            .unwrap()
            .success
    );
    assert!(authenticator.logout().unwrap());
    assert_eq!(authenticator.status(), LoginStatus::LoggedOut);
}
