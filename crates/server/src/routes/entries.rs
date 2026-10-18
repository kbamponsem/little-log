//! Entries, the sync endpoints, the CSV export and "delete everything".

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use little_log_core::model::Entry;
use little_log_core::permissions::{can, Action};
use little_log_core::sync::Cursor;
use little_log_core::{csv, validate, Repo};
use serde::{Deserialize, Serialize};

use crate::auth::{caregiver_id, require_baby, CurrentUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /api/entries` — every live entry, newest first.
pub async fn list(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> ApiResult<Json<Vec<Entry>>> {
    Ok(Json(state.repo().entries().map_err(ApiError::Internal)?))
}

/// `POST /api/entries` — create or update one entry (last-write-wins).
pub async fn upsert(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(mut entry): Json<Entry>,
) -> ApiResult<Json<Entry>> {
    let baby = require_baby(&state)?;
    entry.baby_id = baby.id;
    if entry.caregiver_id.is_none() {
        entry.caregiver_id = Some(caregiver_id(&user));
    }

    let stored = {
        let repo = state.repo();
        ensure_may_write(&repo, &user, &entry)?;
        repo.save_entry(&entry).map_err(ApiError::Internal)?;
        repo.entry(&entry.id)
            .map_err(ApiError::Internal)?
            .ok_or(ApiError::NotFound)?
    };
    Ok(Json(stored))
}

/// `DELETE /api/entries/{id}` — soft delete, so other devices converge.
pub async fn remove(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Entry>> {
    let tombstone = {
        let repo = state.repo();
        let existing = repo
            .entry(&id)
            .map_err(ApiError::Internal)?
            .ok_or(ApiError::NotFound)?;
        let author = caregiver_id(&user);
        let is_author = existing.caregiver_id.as_deref() == Some(author.as_str());
        if !can(user.caregiver.role, Action::DeleteEntry { is_author }) {
            return Err(ApiError::Forbidden("your role cannot delete this entry"));
        }
        repo.soft_delete(&id, Some(&author))
            .map_err(ApiError::Internal)?
    };
    Ok(Json(tombstone))
}

/// `GET /api/export.csv` — the same five columns as the app's export.
pub async fn export(State(state): State<AppState>, _user: CurrentUser) -> ApiResult<Response> {
    let entries = state.repo().entries().map_err(ApiError::Internal)?;
    let body = csv::export(&entries, validate::now_millis());
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"little-log.csv\"",
            ),
        ],
        body,
    )
        .into_response())
}

/// `DELETE /api/data` — tombstone the whole log (owner only).
pub async fn delete_all(State(state): State<AppState>, user: CurrentUser) -> ApiResult<StatusCode> {
    if !can(user.caregiver.role, Action::DeleteAllData) {
        return Err(ApiError::Forbidden("only the owner can delete everything"));
    }
    state.repo().delete_all(None).map_err(ApiError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/* ------------------------------------------------------------------- sync */

#[derive(Deserialize)]
pub struct PullQuery {
    /// `"<updated_at>:<id>"` from the previous pull.
    #[serde(default)]
    pub since: Option<String>,
}

#[derive(Serialize)]
pub struct PullResponse {
    pub entries: Vec<Entry>,
    pub baby: little_log_core::model::Baby,
    /// Pass this back as `?since=` next time.
    pub cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct PushBody {
    pub entries: Vec<Entry>,
}

#[derive(Serialize)]
pub struct PushResponse {
    pub applied: usize,
    pub skipped: usize,
    pub cursor: Option<String>,
}

/// `GET /api/sync?since=<cursor>` — everything that changed, tombstones included.
pub async fn pull(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<PullQuery>,
) -> ApiResult<Json<PullResponse>> {
    let since = match query.since.as_deref() {
        Some(value) if !value.trim().is_empty() => Some(
            Cursor::parse(value.trim())
                .ok_or_else(|| ApiError::bad_request("that sync cursor is not valid"))?,
        ),
        _ => None,
    };

    let baby = require_baby(&state)?;
    let (entries, cursor) = {
        let repo = state.repo();
        let entries = repo
            .entries_since(since.as_ref())
            .map_err(ApiError::Internal)?;
        let cursor = Cursor::latest(&entries).or(since);
        (entries, cursor)
    };

    Ok(Json(PullResponse {
        entries,
        baby,
        cursor: cursor.map(|cursor| cursor.as_string()),
    }))
}

/// `POST /api/sync` — push local changes. Idempotent: re-sending a row that is
/// already stored changes nothing.
pub async fn push(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<PushBody>,
) -> ApiResult<Json<PushResponse>> {
    let baby = require_baby(&state)?;
    let mut applied = 0usize;
    let mut skipped = 0usize;

    {
        let repo = state.repo();
        for mut entry in body.entries {
            entry.baby_id = baby.id.clone();
            if entry.caregiver_id.is_none() {
                entry.caregiver_id = Some(caregiver_id(&user));
            }
            ensure_may_write(&repo, &user, &entry)?;

            // Applying is last-write-wins; an identical re-push reports false.
            if repo.save_remote_entry(&entry).map_err(ApiError::Internal)? {
                applied += 1;
            } else {
                skipped += 1;
            }
        }
    }

    let cursor = state.repo().latest_cursor().map_err(ApiError::Internal)?;
    Ok(Json(PushResponse {
        applied,
        skipped,
        cursor: cursor.map(|cursor| cursor.as_string()),
    }))
}

/* ---------------------------------------------------------------- helpers */

/// Writing is allowed when the role may create, or may edit/delete the row it
/// is trying to change (caregivers can only touch their own).
fn ensure_may_write(repo: &Repo, user: &CurrentUser, entry: &Entry) -> ApiResult<()> {
    let author = caregiver_id(user);
    let action = match repo.entry(&entry.id).map_err(ApiError::Internal)? {
        None => Action::CreateEntry,
        Some(existing) => {
            let is_author = existing.caregiver_id.as_deref() == Some(author.as_str());
            if entry.deleted {
                Action::DeleteEntry { is_author }
            } else {
                Action::EditEntry { is_author }
            }
        }
    };

    if can(user.caregiver.role, action) {
        Ok(())
    } else {
        Err(ApiError::Forbidden("your role cannot change that entry"))
    }
}
