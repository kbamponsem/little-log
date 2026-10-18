//! Little Log's self-hosted sync server.
//!
//! One binary: the JSON API the app and the browser UI talk to, plus the built
//! frontend itself, all backed by the same `little-log-core` the app uses — so
//! validation, last-write-wins and the schema can never drift.

pub mod auth;
pub mod error;
pub mod routes;
pub mod state;

use axum::http::StatusCode;
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Build the whole application: `/api/…` for data, everything else for the UI.
pub fn build_router(state: AppState) -> Router {
    let web_dir = state.config.web_dir.clone();
    let index = web_dir.join("index.html");

    let router = Router::new()
        // Public liveness probe for Docker/monitoring.
        .route("/healthz", axum::routing::get(routes::health::health))
        .nest("/api", routes::api_routes().with_state(state.clone()))
        .with_state(state);

    let router = if index.is_file() {
        // Serve the built frontend, falling back to index.html so deep links work.
        router.fallback_service(ServeDir::new(web_dir).fallback(ServeFile::new(index)))
    } else {
        router.fallback(|| async {
            (
                StatusCode::OK,
                "Little Log API is running. Build the frontend (`npm run build` in web/) to serve the UI.\n",
            )
        })
    };

    router.layer(TraceLayer::new_for_http())
}
