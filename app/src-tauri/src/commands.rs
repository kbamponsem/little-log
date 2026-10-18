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

/// What `auth_me` hands the frontend: who is using this device, and the baby.
#[derive(Serialize)]
pub struct Session {
    pub user: Caregiver,
    pub baby: Baby,
}

#[derive(Serialize)]
pub struct Status {
    pub needs_setup: bool,
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

/* ------------------------------------------------------------------- auth */

/// The app always has a local profile, so setup is never "needed".
#[tauri::command]
pub fn auth_status() -> Status {
    Status { needs_setup: false }
}

#[tauri::command]
pub fn auth_me(state: State<'_, AppState>) -> CmdResult<Session> {
    current_session(&state)
}

/// First run in the app: keep the local profile, remember where the sync
/// server lives. The password itself only ever goes to that server.
#[tauri::command]
pub fn auth_register(
    state: State<'_, AppState>,
    name: String,
    email: String,
    password: String,
    invite: Option<String>,
    server: Option<String>,
) -> CmdResult<Session> {
    let _ = (password, invite); // sent to little-log-server in phase 2 sync
    remember_account(&state, &name, &email, server);
    current_session(&state)
}

/// Signing in on the app records the account locally; the server handshake
/// arrives with `sync_now` (phase 2). Local data is never gated on it.
#[tauri::command]
pub fn auth_login(
    state: State<'_, AppState>,
    email: String,
    password: String,
    server: Option<String>,
) -> CmdResult<Session> {
    let _ = password;
    remember_account(&state, "", &email, server);
    current_session(&state)
}

/// Forget the sync account; the log itself stays on the device.
#[tauri::command]
pub fn auth_logout(state: State<'_, AppState>) -> CmdResult<()> {
    let mut settings = state.settings();
    settings.session_token.clear();
    state.store_settings(settings);
    Ok(())
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
    state
        .repo()
        .baby()
        .map_err(fail)?
        .ok_or_else(|| "no baby profile yet".to_string())
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

/* ---------------------------------------------------------------- helpers */

fn current_session(state: &AppState) -> CmdResult<Session> {
    let repo = state.repo();
    let baby = repo
        .baby()
        .map_err(fail)?
        .ok_or_else(|| "no baby profile yet".to_string())?;
    let user = repo
        .caregivers()
        .map_err(fail)?
        .into_iter()
        .next()
        .ok_or_else(|| "no local profile yet".to_string())?;
    Ok(Session { user, baby })
}

fn remember_account(state: &AppState, name: &str, email: &str, server: Option<String>) {
    let mut settings = state.settings();
    settings.email = email.trim().to_string();
    if let Some(server) = server {
        settings.server = server.trim().to_string();
    }
    state.store_settings(settings);

    let repo = state.repo();
    let Ok(baby) = repo.ensure_baby("Baby") else {
        return;
    };
    let existing = repo
        .caregivers()
        .ok()
        .and_then(|caregivers| caregivers.into_iter().find(|c| c.role == Role::Owner));

    let mut owner = existing.unwrap_or_else(|| {
        let mut owner = Caregiver::new("You", Role::Owner);
        owner.baby_id = baby.id.clone();
        owner
    });
    if !name.trim().is_empty() {
        owner.name = name.trim().to_string();
    }
    owner.baby_id = baby.id;
    owner.email = Some(email.trim().to_string());
    owner.updated_at = validate::now_millis();
    let _ = repo.save_caregiver(&owner);
}
