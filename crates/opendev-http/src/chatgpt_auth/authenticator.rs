use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use base64::Engine;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use url::Url;

use crate::auth::{ChatGptOAuthCredential, CredentialStore};
use crate::models::HttpError;

use super::protocol;

/// Supplies short-lived OAuth headers immediately before each physical send.
#[async_trait]
pub trait RequestAuthenticator: Send + Sync {
    async fn headers_for_request(
        &self,
        cancel: Option<&CancellationToken>,
    ) -> Result<HeaderMap, HttpError>;
    async fn force_refresh(&self, cancel: Option<&CancellationToken>) -> Result<(), HttpError>;
}

/// Browser authorization-code login state. Its verifier stays in memory only.
pub struct BrowserLogin {
    pub authorization_url: String,
    state: String,
    verifier: String,
    listener_ipv4: Option<TcpListener>,
    listener_ipv6: Option<TcpListener>,
}

#[cfg(test)]
impl BrowserLogin {
    pub(crate) fn state_for_test(&self) -> &str {
        &self.state
    }

    pub(crate) fn has_ipv6_listener_for_test(&self) -> bool {
        self.listener_ipv6.is_some()
    }
}

/// Device-login state shown to a headless user. Secrets required to complete
/// the exchange stay private to this module.
pub struct DeviceLogin {
    pub verification_url: String,
    pub user_code: String,
    device_auth_id: String,
    interval: Duration,
}

/// Non-secret local credential state for CLI display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginStatus {
    LoggedOut,
    Active {
        account_id: Option<String>,
        expires_at_ms: i64,
    },
    Expired {
        account_id: Option<String>,
        expires_at_ms: i64,
    },
}

/// Refresh-aware ChatGPT OAuth authenticator. The mutex prevents multiple
/// OpenDev tasks in this process from racing a rotating refresh token.
pub struct ChatGptAuthenticator {
    store: Arc<Mutex<CredentialStore>>,
    refresh_lock: AsyncMutex<()>,
    oauth_client: reqwest::Client,
    token_url: String,
    device_code_url: String,
    device_poll_url: String,
    device_poll_min_interval: u64,
}

impl ChatGptAuthenticator {
    pub fn new(store: CredentialStore) -> Result<Self, HttpError> {
        warn!(
            "ChatGPT OAuth refresh is single-flight within one OpenDev process; avoid concurrent OpenDev processes using the same local credential because refresh tokens may rotate"
        );
        let oauth_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            refresh_lock: AsyncMutex::new(()),
            oauth_client,
            token_url: protocol::TOKEN_URL.to_string(),
            device_code_url: protocol::DEVICE_CODE_URL.to_string(),
            device_poll_url: protocol::DEVICE_POLL_URL.to_string(),
            device_poll_min_interval: protocol::DEVICE_POLL_MIN_INTERVAL_SECS,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_token_url(store: CredentialStore, token_url: String) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            refresh_lock: AsyncMutex::new(()),
            oauth_client: reqwest::Client::new(),
            token_url,
            device_code_url: protocol::DEVICE_CODE_URL.to_string(),
            device_poll_url: protocol::DEVICE_POLL_URL.to_string(),
            device_poll_min_interval: protocol::DEVICE_POLL_MIN_INTERVAL_SECS,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_endpoints(
        store: CredentialStore,
        token_url: String,
        device_code_url: String,
        device_poll_url: String,
    ) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            refresh_lock: AsyncMutex::new(()),
            oauth_client: reqwest::Client::new(),
            token_url,
            device_code_url,
            device_poll_url,
            device_poll_min_interval: 0,
        }
    }

    pub async fn begin_browser_login(&self) -> Result<BrowserLogin, HttpError> {
        // The registered redirect uses `localhost`, which can resolve to
        // either loopback family. Prefer IPv6, then retain IPv4 whenever the
        // platform permits both listeners on the registered port.
        let listener_ipv6 = TcpListener::bind(("::1", protocol::CALLBACK_PORT))
            .await
            .ok();
        let listener_ipv4 = TcpListener::bind((protocol::CALLBACK_HOST, protocol::CALLBACK_PORT))
            .await
            .ok();
        if listener_ipv4.is_none() && listener_ipv6.is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "could not bind the localhost OAuth callback port",
            )
            .into());
        }
        let state = random_urlsafe(32);
        let verifier = random_urlsafe(64);
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        let mut url = Url::parse(protocol::AUTHORIZE_URL)
            .map_err(|e| HttpError::Auth(format!("invalid trusted authorization URL: {e}")))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", protocol::OAUTH_CLIENT_ID)
            .append_pair("redirect_uri", protocol::REDIRECT_URI)
            .append_pair("scope", protocol::SCOPES)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("id_token_add_organizations", "true")
            .append_pair("codex_cli_simplified_flow", "true")
            .append_pair("originator", protocol::ORIGINATOR_HEADER_VALUE);
        Ok(BrowserLogin {
            authorization_url: url.into(),
            state,
            verifier,
            listener_ipv4,
            listener_ipv6,
        })
    }

    /// Start the approved device-login flow. This is not RFC 8628 token
    /// polling: the service returns an authorization code and PKCE verifier
    /// which are exchanged at the normal OAuth token endpoint.
    pub async fn begin_device_login(
        &self,
        cancel: Option<&CancellationToken>,
    ) -> Result<DeviceLogin, HttpError> {
        let response = self
            .request_json(
                &self.device_code_url,
                serde_json::json!({ "client_id": protocol::OAUTH_CLIENT_ID }),
                cancel,
            )
            .await?;
        let device_auth_id = required_string(&response, "device_auth_id", "device login response")?;
        let user_code = response
            .get("user_code")
            .or_else(|| response.get("usercode"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| HttpError::Auth("device login response omitted user_code".into()))?;
        let interval = response
            .get("interval")
            .and_then(Value::as_u64)
            .unwrap_or(self.device_poll_min_interval)
            .max(self.device_poll_min_interval);
        Ok(DeviceLogin {
            verification_url: protocol::DEVICE_VERIFY_URL.to_string(),
            user_code,
            device_auth_id,
            interval: Duration::from_secs(interval),
        })
    }

    /// Poll device authorization until success, cancellation, or a bounded
    /// deadline, then exchange the returned code with its verifier.
    pub async fn finish_device_login(
        &self,
        login: DeviceLogin,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), HttpError> {
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(protocol::DEVICE_LOGIN_TIMEOUT_SECS);
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(HttpError::Auth(
                    "device login timed out; run `opendev setup` and choose OpenAI > ChatGPT Pro/Plus (headless) again".into(),
                ));
            }
            let request = self
                .oauth_client
                .post(&self.device_poll_url)
                .json(&serde_json::json!({
                    "device_auth_id": login.device_auth_id,
                    "user_code": login.user_code,
                }))
                .send();
            let response = match cancel {
                Some(token) => {
                    tokio::select! { value = request => value?, _ = token.cancelled() => return Err(HttpError::Interrupted) }
                }
                None => request.await?,
            };
            match response.status().as_u16() {
                200 => {
                    let response: Value = response.json().await?;
                    let code = required_string(
                        &response,
                        "authorization_code",
                        "device authorization response",
                    )?;
                    let verifier = required_string(
                        &response,
                        "code_verifier",
                        "device authorization response",
                    )?;
                    self.exchange_device_code(&code, &verifier, cancel).await?;
                    return Ok(());
                }
                // These two statuses are the verified pending responses for this
                // profile; do not invent generic OAuth pending/slow_down rules.
                403 | 404 => self.sleep_or_cancel(login.interval, cancel).await?,
                status => {
                    return Err(HttpError::Auth(format!(
                        "device login was rejected (HTTP {status}); enable device login in ChatGPT security settings or ask your workspace administrator"
                    )));
                }
            }
        }
    }

    pub async fn finish_browser_login(
        &self,
        login: BrowserLogin,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), HttpError> {
        let BrowserLogin {
            state,
            verifier,
            listener_ipv4,
            listener_ipv6,
            ..
        } = login;
        let (mut stream, _) = {
            let accept_callback = async {
                match (&listener_ipv4, &listener_ipv6) {
                    (Some(listener_ipv4), Some(listener_ipv6)) => {
                        tokio::select! {
                            accepted = listener_ipv4.accept() => accepted,
                            accepted = listener_ipv6.accept() => accepted,
                        }
                    }
                    (Some(listener_ipv4), None) => listener_ipv4.accept().await,
                    (None, Some(listener_ipv6)) => listener_ipv6.accept().await,
                    (None, None) => unreachable!("browser login has no callback listener"),
                }
            };
            match cancel {
                Some(token) => tokio::select! {
                    accepted = accept_callback => accepted?,
                    _ = token.cancelled() => return Err(HttpError::Interrupted),
                },
                None => accept_callback.await?,
            }
        };
        // The callback was accepted; release both listening sockets before
        // exchanging the authorization code so a subsequent login can start.
        drop(listener_ipv4);
        drop(listener_ipv6);
        let mut request = vec![0_u8; 16 * 1024];
        let size = stream.read(&mut request).await?;
        let raw = std::str::from_utf8(&request[..size])
            .map_err(|_| HttpError::Auth("malformed OAuth callback".into()))?;
        let target = raw
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| HttpError::Auth("malformed OAuth callback".into()))?;
        let callback = Url::parse(&format!("http://localhost{target}"))
            .map_err(|_| HttpError::Auth("malformed OAuth callback".into()))?;
        let pairs: std::collections::HashMap<_, _> = callback.query_pairs().into_owned().collect();
        let code = match (
            callback.path() == protocol::CALLBACK_PATH,
            pairs.get("state"),
            pairs.get("error"),
            pairs.get("code"),
        ) {
            (false, _, _, _) => Err(HttpError::Auth("unexpected OAuth callback path".into())),
            (_, _, Some(error), _) => Err(HttpError::Auth(format!(
                "OAuth authorization was denied: {error}"
            ))),
            (_, Some(callback_state), None, Some(code)) if callback_state == &state => {
                Ok(code.clone())
            }
            _ => Err(HttpError::Auth(
                "OAuth callback state or code was invalid".into(),
            )),
        };
        let body = if code.is_ok() {
            "Login complete. You may close this window."
        } else {
            "Login failed. Return to OpenDev for details."
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
        self.exchange_code(&code?, &verifier, cancel).await
    }

    pub fn status(&self) -> LoginStatus {
        let credential = self
            .store
            .lock()
            .expect("credential mutex poisoned")
            .get_chatgpt_oauth();
        match credential {
            None => LoginStatus::LoggedOut,
            Some(value) if value.expires_at_ms <= now_ms() => LoginStatus::Expired {
                account_id: value.account_id,
                expires_at_ms: value.expires_at_ms,
            },
            Some(value) => LoginStatus::Active {
                account_id: value.account_id,
                expires_at_ms: value.expires_at_ms,
            },
        }
    }

    pub fn logout(&self) -> Result<bool, HttpError> {
        self.store
            .lock()
            .expect("credential mutex poisoned")
            .remove_chatgpt_oauth()
    }

    async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), HttpError> {
        let token = self
            .request_token(
                &[
                    ("grant_type", "authorization_code"),
                    ("client_id", protocol::OAUTH_CLIENT_ID),
                    ("code", code),
                    ("redirect_uri", protocol::REDIRECT_URI),
                    ("code_verifier", verifier),
                ],
                cancel,
            )
            .await?;
        self.store_token_response(token, None)
    }

    async fn exchange_device_code(
        &self,
        code: &str,
        verifier: &str,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), HttpError> {
        let token = self
            .request_token(
                &[
                    ("grant_type", "authorization_code"),
                    ("client_id", protocol::OAUTH_CLIENT_ID),
                    ("code", code),
                    ("redirect_uri", protocol::DEVICE_REDIRECT_URI),
                    ("code_verifier", verifier),
                ],
                cancel,
            )
            .await?;
        self.store_token_response(token, None)
    }

    async fn refresh(&self, cancel: Option<&CancellationToken>) -> Result<(), HttpError> {
        let _guard = self.refresh_lock.lock().await;
        let current = self
            .store
            .lock()
            .expect("credential mutex poisoned")
            .get_chatgpt_oauth()
            .ok_or_else(|| {
                HttpError::Auth(
                    "ChatGPT login is required; run `opendev setup` and choose OpenAI > ChatGPT Pro/Plus".into(),
                )
            })?;
        if !is_expiring(&current) {
            return Ok(());
        }
        let token = self
            .request_token(
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", protocol::OAUTH_CLIENT_ID),
                    ("refresh_token", &current.refresh_token),
                ],
                cancel,
            )
            .await;
        match token {
            Ok(token) => self.store_token_response(token, Some(current)),
            Err(error) => {
                if error.to_string().contains("invalid_grant") {
                    let _ = self.logout();
                    Err(HttpError::Auth(
                        "ChatGPT login expired; run `opendev setup` and choose OpenAI > ChatGPT Pro/Plus again".into(),
                    ))
                } else {
                    Err(error)
                }
            }
        }
    }

    async fn request_token(
        &self,
        form: &[(&str, &str)],
        cancel: Option<&CancellationToken>,
    ) -> Result<TokenResponse, HttpError> {
        let encoded = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form.iter().copied())
            .finish();
        let request = self
            .oauth_client
            .post(&self.token_url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(encoded)
            .send();
        let response = match cancel {
            Some(token) => {
                tokio::select! { value = request => value?, _ = token.cancelled() => return Err(HttpError::Interrupted) }
            }
            None => request.await?,
        };
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(HttpError::Auth(format!(
                "OAuth token request failed ({status}): {}",
                safe_oauth_error(&body)
            )));
        }
        response.json().await.map_err(HttpError::from)
    }

    async fn request_json(
        &self,
        url: &str,
        body: Value,
        cancel: Option<&CancellationToken>,
    ) -> Result<Value, HttpError> {
        let request = self.oauth_client.post(url).json(&body).send();
        let response = match cancel {
            Some(token) => {
                tokio::select! { value = request => value?, _ = token.cancelled() => return Err(HttpError::Interrupted) }
            }
            None => request.await?,
        };
        if !response.status().is_success() {
            return Err(HttpError::Auth(format!(
                "device login request failed (HTTP {})",
                response.status()
            )));
        }
        response.json().await.map_err(HttpError::from)
    }

    async fn sleep_or_cancel(
        &self,
        duration: Duration,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), HttpError> {
        match cancel {
            Some(token) => {
                tokio::select! { _ = tokio::time::sleep(duration) => Ok(()), _ = token.cancelled() => Err(HttpError::Interrupted) }
            }
            None => {
                tokio::time::sleep(duration).await;
                Ok(())
            }
        }
    }

    fn store_token_response(
        &self,
        token: TokenResponse,
        previous: Option<ChatGptOAuthCredential>,
    ) -> Result<(), HttpError> {
        let expires_in = token.expires_in.unwrap_or(3600).clamp(1, 86_400);
        let account_id = token
            .id_token
            .as_deref()
            .and_then(account_claim)
            .or_else(|| account_claim(&token.access_token));
        let credential = ChatGptOAuthCredential {
            access_token: token.access_token,
            refresh_token: token
                .refresh_token
                .or_else(|| previous.map(|value| value.refresh_token))
                .ok_or_else(|| {
                    HttpError::Auth("OAuth token response omitted a refresh token".into())
                })?,
            expires_at_ms: now_ms() + i64::from(expires_in) * 1000,
            account_id,
        };
        self.store
            .lock()
            .expect("credential mutex poisoned")
            .store_chatgpt_oauth(credential)
    }
}

#[async_trait]
impl RequestAuthenticator for ChatGptAuthenticator {
    async fn headers_for_request(
        &self,
        cancel: Option<&CancellationToken>,
    ) -> Result<HeaderMap, HttpError> {
        self.refresh(cancel).await?;
        let credential = self
            .store
            .lock()
            .expect("credential mutex poisoned")
            .get_chatgpt_oauth()
            .ok_or_else(|| {
                HttpError::Auth(
                    "ChatGPT login is required; run `opendev setup` and choose OpenAI > ChatGPT Pro/Plus".into(),
                )
            })?;
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", credential.access_token)).map_err(
                |_| HttpError::Auth("OAuth access token contained invalid header bytes".into()),
            )?,
        );
        headers.insert(
            "openai-beta",
            HeaderValue::from_static(protocol::BETA_HEADER_VALUE),
        );
        headers.insert(
            "originator",
            HeaderValue::from_static(protocol::ORIGINATOR_HEADER_VALUE),
        );
        if let Some(account_id) = credential.account_id
            && let Ok(value) = HeaderValue::from_str(&account_id)
        {
            headers.insert("chatgpt-account-id", value);
        }
        Ok(headers)
    }

    async fn force_refresh(&self, cancel: Option<&CancellationToken>) -> Result<(), HttpError> {
        let current = self
            .store
            .lock()
            .expect("credential mutex poisoned")
            .get_chatgpt_oauth()
            .ok_or_else(|| {
                HttpError::Auth(
                    "ChatGPT login is required; run `opendev setup` and choose OpenAI > ChatGPT Pro/Plus".into(),
                )
            })?;
        // Make the token visibly stale then run the normal single-flight path.
        self.store
            .lock()
            .expect("credential mutex poisoned")
            .store_chatgpt_oauth(ChatGptOAuthCredential {
                expires_at_ms: 0,
                ..current
            })?;
        self.refresh(cancel).await
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i32>,
    id_token: Option<String>,
}

fn random_urlsafe(bytes: usize) -> String {
    let mut raw = vec![0_u8; bytes];
    for chunk in raw.chunks_mut(16) {
        chunk.copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn is_expiring(value: &ChatGptOAuthCredential) -> bool {
    value.expires_at_ms <= now_ms() + protocol::REFRESH_SKEW_SECS * 1000
}
fn safe_oauth_error(body: &str) -> &str {
    if body.contains("invalid_grant") {
        "invalid_grant"
    } else {
        "request rejected"
    }
}
pub(crate) fn account_claim(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    json.get("https://api.openai.com/auth")
        .and_then(|value| {
            value
                .get("chatgpt_account_id")
                .or_else(|| value.get("account_id"))
                .or_else(|| value.get("id"))
        })
        .or_else(|| json.get("account_id"))
        .or_else(|| json.get("organization_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn required_string(value: &Value, field: &str, context: &str) -> Result<String, HttpError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| HttpError::Auth(format!("{context} omitted {field}")))
}
