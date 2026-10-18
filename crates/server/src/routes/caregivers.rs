//! Caregivers and invite links.

use axum::extract::State;
use axum::Json;
use little_log_core::model::{Caregiver, Role};
use little_log_core::permissions::{can, Action};
use little_log_core::validate::now_millis;
use serde::{Deserialize, Serialize};

use crate::auth::{caregiver_id, new_token, token_hash, CurrentUser};
use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, INVITE_TTL_MS};

#[derive(Serialize)]
pub struct InviteResponse {
    /// Send this to the invitee: `<origin>?invite=<token>`.
    pub token: String,
    pub role: Role,
}

#[derive(Deserialize)]
pub struct InviteRequest {
    pub role: Role,
}

/// `GET /api/caregivers` — who else can see this log.
pub async fn list(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> ApiResult<Json<Vec<Caregiver>>> {
    Ok(Json(state.repo().caregivers().map_err(ApiError::Internal)?))
}

/// `POST /api/invites` — mint a single-use link (co-parents and up).
pub async fn create_invite(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<InviteRequest>,
) -> ApiResult<Json<InviteResponse>> {
    if !can(user.caregiver.role, Action::InviteCaregiver) {
        return Err(ApiError::Forbidden(
            "your role cannot invite other caregivers",
        ));
    }

    let token = new_token();
    let hash = token_hash(&state.config.session_secret, &token);
    state
        .repo()
        .create_invite(
            &hash,
            body.role,
            &caregiver_id(&user),
            now_millis() + INVITE_TTL_MS,
        )
        .map_err(ApiError::Internal)?;

    Ok(Json(InviteResponse {
        token,
        role: body.role,
    }))
}
