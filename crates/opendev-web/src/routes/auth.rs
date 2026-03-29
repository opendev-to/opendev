//! Authentication routes.
//!
//! Implements login, register, logout, and get-me endpoints using
//! Argon2 password hashing and HMAC-SHA256 signed tokens stored in
//! HTTP-only cookies.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde::{Deserialize, Serialize};

use crate::error::WebError;
use crate::state::AppState;

/// Cookie name for the session token.
pub const TOKEN_COOKIE: &str = "opendev_session";

/// Token time-to-live in seconds (24 hours).
pub const TOKEN_TTL_SECONDS: i64 = 60 * 60 * 24;

/// Authentication response returned to the client.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub role: String,
}

/// Login request body.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Register request body.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub email: Option<String>,
}

/// Token payload embedded in the HMAC-signed cookie value.
#[derive(Debug, Serialize, Deserialize)]
struct TokenPayload {
    /// Subject (user ID as UUID string).
    sub: String,
    /// Issued-at timestamp (seconds since UNIX epoch).
    iat: i64,
}

/// Secret key for HMAC signing. In production this should come from
/// an environment variable or config; we provide a default for development.
fn secret_key() -> &'static [u8] {
    // Allow override via env at startup. We leak a small allocation so
    // the reference is 'static — acceptable for a single-value config.
    static KEY: std::sync::OnceLock<&'static [u8]> = std::sync::OnceLock::new();
    KEY.get_or_init(|| match std::env::var("OPENDEV_SECRET_KEY") {
        Ok(val) => Box::leak(val.into_bytes().into_boxed_slice()) as &[u8],
        Err(_) => b"change-me-in-production",
    })
}

/// Create an HMAC-SHA256 signed token encoding the user ID.
pub fn create_token(user_id: &str) -> String {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let payload = TokenPayload {
        sub: user_id.to_string(),
        iat: chrono::Utc::now().timestamp(),
    };
    let payload_json = serde_json::to_string(&payload).expect("serialize token payload");
    let payload_b64 =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json.as_bytes());

    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret_key()).expect("HMAC can take key of any size");
    mac.update(payload_b64.as_bytes());
    let sig = mac.finalize().into_bytes();
    let sig_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sig);

    format!("{}.{}", payload_b64, sig_b64)
}

/// Verify an HMAC-SHA256 signed token and return the user ID (subject).
///
/// Returns `Err(WebError::Unauthorized)` if the token is invalid or expired.
pub fn verify_token(token: &str) -> Result<String, WebError> {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let parts: Vec<&str> = token.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err(WebError::Unauthorized("Invalid token format".to_string()));
    }

    let (payload_b64, sig_b64) = (parts[0], parts[1]);

    // Verify HMAC.
    let sig_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| WebError::Unauthorized("Invalid token signature encoding".to_string()))?;

    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret_key()).expect("HMAC can take key of any size");
    mac.update(payload_b64.as_bytes());
    mac.verify_slice(&sig_bytes)
        .map_err(|_| WebError::Unauthorized("Invalid token signature".to_string()))?;

    // Decode payload.
    let payload_json = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| WebError::Unauthorized("Invalid token payload encoding".to_string()))?;
    let payload: TokenPayload = serde_json::from_slice(&payload_json)
        .map_err(|_| WebError::Unauthorized("Invalid token payload".to_string()))?;

    // Check TTL.
    let now = chrono::Utc::now().timestamp();
    if now - payload.iat > TOKEN_TTL_SECONDS {
        return Err(WebError::Unauthorized("Token expired".to_string()));
    }

    Ok(payload.sub)
}

/// Hash a password using Argon2id.
fn hash_password(password: &str) -> Result<String, WebError> {
    use argon2::{Argon2, PasswordHasher};
    use password_hash::SaltString;
    use rand_core::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| WebError::Internal(format!("Password hashing failed: {}", e)))?;
    Ok(hash.to_string())
}

/// Verify a password against an Argon2id hash.
fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::password_hash::PasswordHash;
    use argon2::{Argon2, PasswordVerifier};

    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Build a cookie for the session token.
fn build_session_cookie(token: &str) -> Cookie<'static> {
    Cookie::build((TOKEN_COOKIE.to_string(), token.to_string()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(TOKEN_TTL_SECONDS))
        .path("/")
        .build()
}

/// Build the auth router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/register", post(register))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(get_me))
}

/// Login handler.
///
/// Verifies credentials against the UserStore, generates a signed token,
/// and sets an HTTP-only cookie.
async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), WebError> {
    let user_store = state.user_store();

    let user = user_store
        .get_by_username(&payload.username)
        .ok_or_else(|| WebError::BadRequest("Incorrect username or password".to_string()))?;

    if !verify_password(&payload.password, &user.password_hash) {
        return Err(WebError::BadRequest(
            "Incorrect username or password".to_string(),
        ));
    }

    let token = create_token(&user.id.to_string());
    let cookie = build_session_cookie(&token);
    let jar = jar.add(cookie);

    Ok((
        jar,
        Json(AuthResponse {
            username: user.username,
            email: user.email,
            role: user.role,
        }),
    ))
}

/// Register handler.
///
/// Creates a new user with Argon2-hashed password, generates a signed token,
/// and sets an HTTP-only cookie.
async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<RegisterRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), WebError> {
    let user_store = state.user_store();

    if user_store.get_by_username(&payload.username).is_some() {
        return Err(WebError::BadRequest("Username already exists".to_string()));
    }

    let hashed = hash_password(&payload.password)?;
    let user = user_store
        .create_user(&payload.username, &hashed, payload.email.as_deref())
        .map_err(|e| WebError::Internal(format!("Failed to create user: {}", e)))?;

    let token = create_token(&user.id.to_string());
    let cookie = build_session_cookie(&token);
    let jar = jar.add(cookie);

    Ok((
        jar,
        Json(AuthResponse {
            username: user.username,
            email: user.email,
            role: user.role,
        }),
    ))
}

/// Logout handler.
///
/// Clears the session cookie.
async fn logout(jar: CookieJar) -> (CookieJar, Json<serde_json::Value>) {
    let jar = jar.remove(Cookie::from(TOKEN_COOKIE));
    (jar, Json(serde_json::json!({"status": "success"})))
}

/// Get current user from session cookie.
async fn get_me(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<AuthResponse>, WebError> {
    let token = jar
        .get(TOKEN_COOKIE)
        .ok_or_else(|| WebError::Unauthorized("Not authenticated".to_string()))?;

    let user_id_str = verify_token(token.value())?;
    let user_id: uuid::Uuid = user_id_str
        .parse()
        .map_err(|_| WebError::Unauthorized("Invalid user ID in token".to_string()))?;

    let user_store = state.user_store();
    let user = user_store
        .get_by_id(user_id)
        .ok_or_else(|| WebError::Unauthorized("User not found".to_string()))?;

    Ok(Json(AuthResponse {
        username: user.username,
        email: user.email,
        role: user.role,
    }))
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
