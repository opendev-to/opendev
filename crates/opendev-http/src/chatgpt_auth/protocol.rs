//! Source-pinned protocol compatibility profile for ChatGPT Codex OAuth.
//!
//! These are deliberately constants rather than OpenDev settings: accepting
//! user supplied issuer, token, or API URLs would permit credential exfiltration.

/// Approved public OAuth compatibility client profile.
pub const OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
pub const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const DEVICE_CODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
pub const DEVICE_POLL_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
pub const DEVICE_VERIFY_URL: &str = "https://auth.openai.com/codex/device";
pub const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
pub const CALLBACK_HOST: &str = "127.0.0.1";
pub const CALLBACK_PORT: u16 = 1455;
pub const CALLBACK_PATH: &str = "/auth/callback";
pub const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
pub const SCOPES: &str = "openid profile email offline_access";

/// Trusted Responses endpoint used by the ChatGPT Codex backend.
pub const CHATGPT_CODEX_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
/// Fixed credential-store namespace for this transport mode.
pub const CHATGPT_OAUTH_CREDENTIAL_KEY: &str = "openai-chatgpt";
/// Refresh slightly before expiry so a request never starts with a stale token.
pub const REFRESH_SKEW_SECS: i64 = 60;
pub const DEVICE_LOGIN_TIMEOUT_SECS: u64 = 15 * 60;
pub const DEVICE_POLL_MIN_INTERVAL_SECS: u64 = 5;

/// Required static, non-secret protocol headers.
pub const BETA_HEADER_VALUE: &str = "responses=experimental";
pub const ORIGINATOR_HEADER_VALUE: &str = "codex_cli_rs";
