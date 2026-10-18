//! Tauri commands — the app's side of the storage-adapter contract that
//! `web/src/data/storage.ts` (TauriAdapter) calls.
//!
//! Everything runs against the on-device SQLite file, so the app is fully
//! usable offline. Commands never panic: they return a `String` error the UI
//! can show.

use base64::Engine;
use little_log_core::model::{Baby, Caregiver, Entry, Role};
use little_log_core::{csv, photo, validate, CoreError};
use serde::Serialize;
use tauri::State;

use crate::AppState;

/// Commands report failures as plain strings (Tauri needs a serialisable error).
type CmdResult<T> = Result<T, String>;

fn fail(error: CoreError) -> String {
    error.to_string()
}

/// What `device_identity` hands the frontend: this device's id.
///
/// There is no login. The id is the hex of the device's ed25519 public key,
/// generated the first time the app opens — see `little_log_core::keystore`.
#[derive(Serialize)]
pub struct Identity {
    pub id: String,
}

#[derive(Serialize)]
pub struct Invite {
    pub token: String,
    pub role: Role,
}

#[derive(Serialize)]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    pub cursor: Option<String>,
    pub message: String,
}

/* --------------------------------------------------------------- identity */

/// This device's identity — no login, no account, no password.
///
/// Generated on first open and kept on the device; every event it authors is
/// signed with the matching key.
#[tauri::command]
pub fn device_identity(state: State<'_, AppState>) -> CmdResult<Identity> {
    let id = state.repo().device_key().map_err(fail)?.id();
    Ok(Identity { id })
}

/* ---------------------------------------------------------------- entries */

/// Every live entry, newest first.
#[tauri::command]
pub fn list_entries(state: State<'_, AppState>) -> CmdResult<Vec<Entry>> {
    state.repo().entries().map_err(fail)
}

/// Upsert one entry (last-write-wins). Fills in the baby and author.
#[tauri::command]
pub fn save_entry(state: State<'_, AppState>, entry: Entry) -> CmdResult<Entry> {
    let repo = state.repo();
    let mut entry = entry;

    if entry.baby_id.is_none() {
        entry.baby_id = repo.baby().map_err(fail)?.and_then(|baby| baby.id);
    }
    if entry.caregiver_id.is_none() {
        entry.caregiver_id = repo
            .caregivers()
            .map_err(fail)?
            .first()
            .and_then(|caregiver| caregiver.id.clone());
    }

    repo.save_entry(&entry).map_err(fail)?;
    // Return what is actually stored — a stale write keeps the newer row.
    repo.entry(&entry.id)
        .map_err(fail)?
        .ok_or_else(|| CoreError::NotFound(entry.id.clone()).to_string())
}

/// Soft-delete one entry (a tombstone, so other devices converge).
#[tauri::command]
pub fn delete_entry(state: State<'_, AppState>, entry_id: String) -> CmdResult<Entry> {
    let author = state
        .repo()
        .caregivers()
        .map_err(fail)?
        .first()
        .and_then(|caregiver| caregiver.id.clone());
    state
        .repo()
        .soft_delete(&entry_id, author.as_deref())
        .map_err(fail)
}

/* ------------------------------------------------------------------- baby */

#[tauri::command]
pub fn get_baby(state: State<'_, AppState>) -> CmdResult<Baby> {
    // First run has no profile yet; create the first baby so the app can open
    // straight onto Today. Renaming happens on the Baby screen.
    state.repo().ensure_baby("Baby").map_err(fail)
}

#[tauri::command]
pub fn save_baby(state: State<'_, AppState>, baby: Baby) -> CmdResult<Baby> {
    state.repo().save_baby(&baby).map_err(fail)
}

#[tauri::command]
pub fn list_caregivers(state: State<'_, AppState>) -> CmdResult<Vec<Caregiver>> {
    state.repo().caregivers().map_err(fail)
}

/// Invite links are minted by the sync server, so this needs one configured.
#[tauri::command]
pub fn create_invite(state: State<'_, AppState>, role: Role) -> CmdResult<Invite> {
    if !state.settings().has_server() {
        return Err(
            "Connect a sync server in Baby → Caregivers before inviting anyone.".to_string(),
        );
    }
    let _ = role;
    Err("Invites arrive with sync (little-log-server)".to_string())
}

/* ------------------------------------------------------------------- data */

/// `time,type,what,details,notes` — the same export as the prototype.
#[tauri::command]
pub fn export_csv(state: State<'_, AppState>) -> CmdResult<String> {
    let entries = state.repo().entries().map_err(fail)?;
    Ok(csv::export(&entries, validate::now_millis()))
}

/// Soft-delete everything (tombstones), so other devices empty out too.
#[tauri::command]
pub fn delete_all(state: State<'_, AppState>) -> CmdResult<usize> {
    let author = state
        .repo()
        .caregivers()
        .map_err(fail)?
        .first()
        .and_then(|caregiver| caregiver.id.clone());
    state.repo().delete_all(author.as_deref()).map_err(fail)
}

/* ----------------------------------------------------------------- photos */

/// Store a picked photo: downscaled to ≤1280 px JPEG in `little-log-core`.
/// Returns the file name to record on the entry.
#[tauri::command]
pub fn save_photo(
    state: State<'_, AppState>,
    entry_id: String,
    bytes: Vec<u8>,
) -> CmdResult<String> {
    photo::write_photo(&state.photos_dir(), &entry_id, &bytes).map_err(fail)
}

/// Photo bytes as a data URL, for `<img>` in the WebView.
#[tauri::command]
pub fn photo_data_url(state: State<'_, AppState>, entry_id: String) -> CmdResult<String> {
    let bytes = photo::read_photo(&state.photos_dir(), &entry_id).map_err(fail)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:image/jpeg;base64,{encoded}"))
}

/* ------------------------------------------------------------------- sync */

/// Push local changes and pull remote ones. Wired to little-log-server in
/// phase 2; until then this reports clearly instead of failing.
#[tauri::command]
pub fn sync_now(state: State<'_, AppState>) -> CmdResult<SyncReport> {
    let settings = state.settings();
    if !settings.has_server() {
        return Ok(SyncReport {
            pushed: 0,
            pulled: 0,
            cursor: None,
            message: "No sync server configured".to_string(),
        });
    }

    let cursor = state.repo().latest_cursor().map_err(fail)?;
    Ok(SyncReport {
        pushed: 0,
        pulled: 0,
        cursor: cursor.map(|cursor| cursor.as_string()),
        message: format!(
            "{} is unreachable until little-log-server ships",
            settings.server
        ),
    })
}
