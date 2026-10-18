//! Passwords, session tokens and cookies.
//!
//! * Passwords: Argon2id, stored as a PHC string.
//! * Session tokens: 256 bits of randomness, sent as an `HttpOnly; SameSite=Lax`
//!   cookie. Only `HMAC-SHA256(SESSION_SECRET, token)` is stored, so a leaked
//!   database does not hand out live sessions.
//! * Invite tokens: hashed the same way.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use little_log_core::model::{Baby, Caregiver};
use sha2::Sha256;

use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, SESSION_TTL};

pub const SESSION_COOKIE: &str = "ll_session";
/// Argon2id with the defaults the `argon2` crate ships (19 MiB, t=2, p=1).
fn argon2() -> argon2::Argon2<'static> {
    argon2::Argon2::default()
}

/// Hash a password for storage (Argon2id, random salt, PHC string).
pub fn hash_password(password: &str) -> ApiResult<String> {
    use argon2::password_hash::PasswordHasher;

    argon2()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|error| ApiError::Internal(little_log_core::CoreError::data(error.to_string())))
}

/// Check a password against a stored PHC string.
pub fn verify_password(password: &str, phc: &str) -> bool {
    use argon2::password_hash::{phc::PasswordHash, PasswordVerifier};

    match PasswordHash::new(phc) {
        Ok(parsed) => argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// 256 bits of randomness, URL-safe — for session and invite tokens.
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    // `getrandom` failing means the platform RNG is broken: refusing to invent
    // a weaker token is the safe answer.
    if getrandom::fill(&mut bytes).is_err() {
        return String::new();
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// `HMAC-SHA256(secret, token)` as hex — what we store and look up.
pub fn token_hash(secret: &str, token: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(token.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Email sanity check (deliberately loose — this is a household server).
pub fn normalise_email(email: &str) -> ApiResult<String> {
    let email = email.trim().to_lowercase();
    let valid = email.len() >= 3
        && email.contains('@')
        && !email.starts_with('@')
        && !email.ends_with('@')
        && !email.contains(' ');
    if valid {
        Ok(email)
    } else {
        Err(ApiError::bad_request("that email address looks wrong"))
    }
}

pub fn check_password_strength(password: &str) -> ApiResult<()> {
    if password.chars().count() < 8 {
        return Err(ApiError::bad_request(
            "please choose a password of at least 8 characters",
        ));
    }
    Ok(())
}

/* ------------------------------------------------------------------- cookies */

pub fn session_cookie(token: String, secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::seconds(SESSION_TTL.as_secs() as i64))
        .build()
}

pub fn clear_session_cookie(secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::seconds(0))
        .build()
}

/// Cookies are marked `Secure` when the config demands it or the request came
/// in over HTTPS (through a proxy such as Caddy or Tailscale).
pub fn request_is_secure(headers: &HeaderMap, cookie_secure: bool) -> bool {
    if cookie_secure {
        return true;
    }
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').next().unwrap_or("").trim() == "https")
        .unwrap_or(false)
}

/// Best-effort client address for login throttling: the proxy's header when
/// present, otherwise a shared bucket (fine for a household server).
pub fn client_address(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local".to_string())
}

/* -------------------------------------------------------------- extractor */

/// The signed-in caregiver, or a 401.
pub struct CurrentUser {
    pub caregiver: Caregiver,
    pub token_hash: String,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let jar = axum_extra::extract::CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|cookie| cookie.value().to_string())
            .ok_or(ApiError::Unauthorized("please sign in"))?;

        let token_hash = token_hash(&state.config.session_secret, &token);
        let caregiver = state
            .repo()
            .session_caregiver(&token_hash)
            .map_err(ApiError::Internal)?
            .ok_or(ApiError::Unauthorized("your session has expired"))?;

        Ok(CurrentUser {
            caregiver,
            token_hash,
        })
    }
}

/// The signed-in caregiver's id (never `None` for a stored row).
pub fn caregiver_id(user: &CurrentUser) -> String {
    user.caregiver
        .id
        .clone()
        .unwrap_or_else(|| String::from("unknown"))
}

/// The single household baby.
pub fn require_baby(state: &AppState) -> ApiResult<Baby> {
    state
        .repo()
        .baby()
        .map_err(ApiError::Internal)?
        .ok_or(ApiError::NotFound)
}
