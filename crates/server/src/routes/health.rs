//! Liveness probe for Docker's `HEALTHCHECK` and any uptime monitor.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Serialize)]
pub struct Health {
    pub status: &'static str,
    /// Touching the database means a broken volume shows up as unhealthy.
    pub entries: i64,
    pub version: &'static str,
}

/// `GET /healthz`
pub async fn health(State(state): State<AppState>) -> ApiResult<Json<Health>> {
    let entries = state.repo().count_entries().map_err(ApiError::Internal)?;
    Ok(Json(Health {
        status: "ok",
        entries,
        version: env!("CARGO_PKG_VERSION"),
    }))
}
