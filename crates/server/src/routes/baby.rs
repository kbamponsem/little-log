//! The baby profile.

use axum::extract::State;
use axum::Json;
use little_log_core::model::Baby;
use little_log_core::permissions::{can, Action};

use crate::auth::{require_baby, CurrentUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /api/baby`
pub async fn get(State(state): State<AppState>, _user: CurrentUser) -> ApiResult<Json<Baby>> {
    Ok(Json(require_baby(&state)?))
}

/// `PUT /api/baby` — name, birthday and theme (co-parents and up).
pub async fn put(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(baby): Json<Baby>,
) -> ApiResult<Json<Baby>> {
    if !can(user.caregiver.role, Action::ManageBaby) {
        return Err(ApiError::Forbidden(
            "your role cannot change the baby profile",
        ));
    }
    let saved = state.repo().save_baby(&baby).map_err(ApiError::Internal)?;
    Ok(Json(saved))
}
