//! The HTTP surface. Every route here is also exercised in `tests/api.rs`.

pub mod auth;
pub mod baby;
pub mod caregivers;
pub mod entries;
pub mod health;
pub mod photos;

use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post};
use axum::Router;

use crate::state::AppState;

/// `/api/…` — aside from `auth/status`, `auth/register` and `auth/login`,
/// everything needs a session cookie.
pub fn api_routes() -> Router<AppState> {
    Router::new()
        // auth
        .route("/auth/status", get(auth::status))
        .route("/auth/me", get(auth::me))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        // entries
        .route("/entries", get(entries::list).post(entries::upsert))
        .route("/entries/{id}", delete(entries::remove))
        // sync
        .route("/sync", get(entries::pull).post(entries::push))
        // profile
        .route("/baby", get(baby::get).put(baby::put))
        .route("/caregivers", get(caregivers::list))
        .route("/invites", post(caregivers::create_invite))
        // data
        .route("/export.csv", get(entries::export))
        .route("/data", delete(entries::delete_all))
        .route(
            "/photos/{id}",
            get(photos::download)
                .put(photos::upload)
                .layer(DefaultBodyLimit::max(photos::MAX_PHOTO_BYTES)),
        )
}
