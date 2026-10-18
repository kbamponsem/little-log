//! Sign-up, sign-in and sign-out.

use std::sync::OnceLock;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::cookie::CookieJar;
use little_log_core::model::{new_id, Baby, Caregiver, Role};
use little_log_core::validate::now_millis;
use serde::{Deserialize, Serialize};

use crate::auth::{
    check_password_strength, clear_session_cookie, client_address, hash_password, new_token,
    normalise_email, request_is_secure, session_cookie, token_hash, verify_password, CurrentUser,
};
use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, SESSION_TTL};

#[derive(Serialize)]
pub struct Session {
    pub user: Caregiver,
    pub baby: Baby,
}

#[derive(Serialize)]
pub struct Status {
    /// True while the server has no accounts: the first sign-up becomes owner.
    pub needs_setup: bool,
}

#[derive(Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    pub email: String,
    pub password: String,
    /// Required unless this is the first account.
    #[serde(default)]
    pub invite: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// `GET /api/auth/status` — public, so the UI can offer "create account" on a
/// fresh server and "sign in" everywhere else.
pub async fn status(State(state): State<AppState>) -> ApiResult<Json<Status>> {
    let caregivers = state
        .repo()
        .count_caregivers()
        .map_err(ApiError::Internal)?;
    Ok(Json(Status {
        needs_setup: caregivers == 0,
    }))
}

/// `GET /api/auth/me`
pub async fn me(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Session>> {
    let baby = state
        .repo()
        .baby()
        .map_err(ApiError::Internal)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(Session {
        user: user.caregiver,
        baby,
    }))
}

/// `POST /api/auth/register` — first account becomes the owner; everyone else
/// needs a valid invite link.
pub async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(body): Json<RegisterRequest>,
) -> ApiResult<Response> {
    let email = normalise_email(&body.email)?;
    check_password_strength(&body.password)?;
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("please tell us your name"));
    }
    let password_hash = hash_password(&body.password)?;

    // Everything that touches the database happens in one scope, so the lock is
    // released before we open a session.
    let (caregiver_id, baby) = {
        let repo = state.repo();
        let is_first = repo.count_caregivers().map_err(ApiError::Internal)? == 0;

        if is_first {
            let baby = repo.ensure_baby("Baby").map_err(ApiError::Internal)?;
            let mut owner = Caregiver::new(&name, Role::Owner);
            owner.baby_id = baby.id.clone();
            owner.email = Some(email);
            owner.password_hash = Some(password_hash);
            let owner = repo.save_caregiver(&owner).map_err(ApiError::Internal)?;
            let id = owner
                .id
                .clone()
                .ok_or_else(|| ApiError::Internal(little_log_core::CoreError::data("no id")))?;
            (id, baby)
        } else {
            let token = body
                .invite
                .as_deref()
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .ok_or(ApiError::Forbidden(
                    "this server already has an account — ask for an invite link",
                ))?;
            let hash = token_hash(&state.config.session_secret, token);
            let invite = repo
                .invite(&hash)
                .map_err(ApiError::Internal)?
                .ok_or_else(|| ApiError::bad_request("that invite link is not valid"))?;
            if invite.is_redeemed() {
                return Err(ApiError::bad_request(
                    "that invite link has already been used",
                ));
            }
            if invite.is_expired(now_millis()) {
                return Err(ApiError::bad_request("that invite link has expired"));
            }
            if repo
                .caregiver_by_email(&email)
                .map_err(ApiError::Internal)?
                .is_some()
            {
                return Err(ApiError::bad_request("that email is already registered"));
            }

            let caregiver_id = new_id();
            // Consume the invite first: two people cannot race the same link.
            let role = repo
                .accept_invite(&hash, &caregiver_id)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?;
            let baby = repo
                .baby()
                .map_err(ApiError::Internal)?
                .ok_or(ApiError::NotFound)?;

            let mut caregiver = Caregiver::new(&name, role);
            caregiver.id = Some(caregiver_id.clone());
            caregiver.baby_id = baby.id.clone();
            caregiver.email = Some(email);
            caregiver.password_hash = Some(password_hash);
            repo.save_caregiver(&caregiver)
                .map_err(ApiError::Internal)?;

            (caregiver_id, baby)
        }
    };

    finish_sign_in(&state, &jar, &headers, &caregiver_id, baby)
}

/// `POST /api/auth/login`
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> ApiResult<Response> {
    let address = client_address(&headers);
    if state.is_rate_limited(&address) {
        return Err(ApiError::TooManyRequests);
    }

    let email = normalise_email(&body.email)?;
    let found = state
        .repo()
        .password_hash_for(&email)
        .map_err(ApiError::Internal)?;

    let (ok, caregiver) = match found {
        Some((hash, caregiver)) => (verify_password(&body.password, &hash), Some(caregiver)),
        // Hash anyway, so an unknown email and a wrong password take the same
        // amount of time.
        None => {
            let _ = verify_password(&body.password, dummy_hash());
            (false, None)
        }
    };

    let Some(caregiver) = caregiver.filter(|_| ok) else {
        if !state.record_failed_login(&address) {
            return Err(ApiError::TooManyRequests);
        }
        return Err(ApiError::Unauthorized("email or password is wrong"));
    };

    state.clear_attempts(&address);
    let caregiver_id = caregiver
        .id
        .clone()
        .ok_or_else(|| ApiError::Internal(little_log_core::CoreError::data("no id")))?;
    let baby = state
        .repo()
        .baby()
        .map_err(ApiError::Internal)?
        .ok_or(ApiError::NotFound)?;

    finish_sign_in(&state, &jar, &headers, &caregiver_id, baby)
}

/// `POST /api/auth/logout`
pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    user: CurrentUser,
) -> ApiResult<Response> {
    state
        .repo()
        .delete_session(&user.token_hash)
        .map_err(ApiError::Internal)?;
    let secure = request_is_secure(&headers, state.config.cookie_secure);
    Ok((
        jar.add(clear_session_cookie(secure)),
        Json(OkResponse { ok: true }),
    )
        .into_response())
}

/* ---------------------------------------------------------------- helpers */

/// Open a session and hand the family back to the caller.
fn finish_sign_in(
    state: &AppState,
    jar: &CookieJar,
    headers: &HeaderMap,
    caregiver_id: &str,
    baby: Baby,
) -> ApiResult<Response> {
    let token = new_token();
    let hash = token_hash(&state.config.session_secret, &token);
    let expires = now_millis() + SESSION_TTL.as_millis() as i64;

    let caregiver = {
        let repo = state.repo();
        repo.create_session(&hash, caregiver_id, expires)
            .map_err(ApiError::Internal)?;
        repo.caregiver(caregiver_id)
            .map_err(ApiError::Internal)?
            .ok_or(ApiError::NotFound)?
    };

    let secure = request_is_secure(headers, state.config.cookie_secure);
    Ok((
        jar.clone().add(session_cookie(token, secure)),
        Json(Session {
            user: caregiver,
            baby,
        }),
    )
        .into_response())
}

/// A real hash of a throwaway password, computed once, so sign-in timing does
/// not reveal whether an email exists.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| {
        hash_password("little-log-timing-equaliser").unwrap_or_else(|_| String::new())
    })
}
