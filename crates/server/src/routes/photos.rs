//! Diaper photos: stored on disk, resized by the core, served behind the cookie.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use little_log_core::photo;
use serde::Serialize;

use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Photos are downscaled before they reach the request body limit.
pub const MAX_PHOTO_BYTES: usize = 12 * 1024 * 1024;

#[derive(Serialize)]
pub struct PhotoResponse {
    /// The file name to store on the entry (`"<entry id>.jpg"`).
    pub photo: String,
}

/// `PUT /api/photos/{entryId}` — raw JPEG/PNG bytes in, JPEG on disk out.
///
/// The client uploads first and then saves the entry, so this only needs the
/// right to add entries.
pub async fn upload(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(entry_id): Path<String>,
    body: Bytes,
) -> ApiResult<Json<PhotoResponse>> {
    ensure_uuid(&entry_id)?;
    if body.is_empty() {
        return Err(ApiError::bad_request("the photo was empty"));
    }

    let name = photo::write_photo(&state.config.photos(), &entry_id, &body)?;
    Ok(Json(PhotoResponse { photo: name }))
}

/// `GET /api/photos/{entryId}`
pub async fn download(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(entry_id): Path<String>,
) -> ApiResult<Response> {
    ensure_uuid(&entry_id)?;
    let bytes = match photo::read_photo(&state.config.photos(), &entry_id) {
        Ok(bytes) => bytes,
        Err(_) => return Err(ApiError::NotFound),
    };

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "private, max-age=60"),
        ],
        bytes,
    )
        .into_response())
}

/// Ids come from the client, so they are checked before touching the disk.
fn ensure_uuid(id: &str) -> ApiResult<()> {
    let valid = id.len() == 36
        && id
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-');
    if valid {
        Ok(())
    } else {
        Err(ApiError::bad_request("that entry id is not valid"))
    }
}
