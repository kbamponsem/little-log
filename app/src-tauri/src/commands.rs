//! Tauri commands — the app's side of the storage-adapter contract that
//! `web/src/data/storage.ts` (TauriAdapter) calls.
//!
//! Everything runs against the on-device SQLite file, so the app is fully
//! usable offline. Commands never panic: they return a `String` error the UI
//! can show.

use base64::Engine;
use little_log_core::model::{Baby, Caregiver, Entry, JoinState, MoodSchedule, Role, Theme};
use little_log_core::repo::JoinRequest;
use little_log_core::{csv, invite, photo, validate, CoreError};
use serde::Serialize;
use tauri::State;

use crate::sync::SyncStatus;
use crate::widget;
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

/// This device's appearance preference. Never replicated.
#[tauri::command]
pub fn device_theme(state: State<'_, AppState>) -> CmdResult<Theme> {
    state.repo().device_theme().map_err(fail)
}

#[tauri::command]
pub fn save_device_theme(state: State<'_, AppState>, theme: Theme) -> CmdResult<()> {
    state.repo().save_device_theme(theme).map_err(fail)
}

/// This device's daily mood check-in reminder. Local; never replicated.
#[tauri::command]
pub fn device_mood_schedule(state: State<'_, AppState>) -> CmdResult<MoodSchedule> {
    state.repo().mood_schedule().map_err(fail)
}

#[tauri::command]
pub fn save_device_mood_schedule(
    state: State<'_, AppState>,
    schedule: MoodSchedule,
) -> CmdResult<()> {
    state.repo().save_device_mood_schedule(&schedule).map_err(fail)
}

/* ---------------------------------------------------------------- entries */

/// Every live entry for the child this device is looking at, newest first.
///
/// Scoped, not global: the screens show one child, so handing them every
/// child's rows would make "today" count a sibling's feeds.
#[tauri::command]
pub fn list_entries(state: State<'_, AppState>) -> CmdResult<Vec<Entry>> {
    let repo = state.repo();
    let Some(baby_id) = repo.current_baby().map_err(fail)?.and_then(|baby| baby.id) else {
        return Ok(Vec::new());
    };
    repo.entries_for(&baby_id).map_err(fail)
}

/// Log one entry: author an event, fold the row, then catch up with peers.
///
/// The row is a *result* of the event now, never written directly — which is what
/// makes what this device shows and what a peer receives the same thing by
/// construction.
#[tauri::command]
pub fn save_entry(state: State<'_, AppState>, entry: Entry) -> CmdResult<Entry> {
    let mut entry = entry;

    // Carry the photo in the event: a peer has no access to this device's
    // photos/ directory, so the event is what makes the image travel with it.
    // Read it before taking the repo lock; it is file I/O, not database work.
    let photo = if entry.photo.is_some() {
        photo::read_photo(&state.photos_dir(), &entry.id).ok()
    } else {
        None
    };

    let repo = state.repo();

    if entry.baby_id.is_none() {
        entry.baby_id = repo.current_baby().map_err(fail)?.and_then(|baby| baby.id);
    }

    let key = repo.device_key().map_err(fail)?;
    // Seeded from the log's newest head, so a restart cannot author a write that
    // sorts before something already stored.
    let mut clock = repo.clock(key.id()).map_err(fail)?;
    let stored = repo
        .record_new_entry_with_photo(&key, &mut clock, &entry, photo.as_deref())
        .map_err(fail)?;
    drop(repo);

    // A round is a catch-up, not something the parent waits on.
    state.sync_in_background();
    Ok(stored)
}

/// Tombstone one entry, as an event, so other devices converge on the delete.
#[tauri::command]
pub fn delete_entry(state: State<'_, AppState>, entry_id: String) -> CmdResult<Entry> {
    let repo = state.repo();
    let key = repo.device_key().map_err(fail)?;
    let mut clock = repo.clock(key.id()).map_err(fail)?;
    repo.record_delete(&key, &mut clock, &entry_id)
        .map_err(fail)?;
    let stored = repo
        .entry(&entry_id)
        .map_err(fail)?
        .ok_or_else(|| CoreError::NotFound(entry_id.clone()).to_string())?;
    drop(repo);

    state.sync_in_background();
    Ok(stored)
}

/* ----------------------------------------------------------------- babies */

/// Every child in this log, oldest first.
#[tauri::command]
pub fn list_babies(state: State<'_, AppState>) -> CmdResult<Vec<Baby>> {
    state.repo().babies().map_err(fail)
}

/// The child this device is looking at, or `None` when the log holds none yet
/// — which is exactly when the app should ask for one.
#[tauri::command]
pub fn current_baby(state: State<'_, AppState>) -> CmdResult<Option<Baby>> {
    state.repo().current_baby().map_err(fail)
}

/// Add a child: a fresh id, a fresh group key and a first profile event, so
/// everything logged about it is sealed and can be handed to another device.
///
/// The new child also becomes the one this device is looking at, because you
/// have just been looking at its name.
#[tauri::command]
pub fn create_baby(state: State<'_, AppState>, name: String, born: String) -> CmdResult<Baby> {
    let repo = state.repo();
    let key = repo.device_key().map_err(fail)?;
    let mut clock = repo.clock(key.id()).map_err(fail)?;

    let baby = repo
        .create_baby(&key, &mut clock, &name, &born)
        .map_err(fail)?;
    if let Some(id) = baby.id.as_deref() {
        repo.set_current_baby(id).map_err(fail)?;
    }
    Ok(baby)
}

/// Save a child's name, birthday and picture as a profile write, so the change
/// reaches the other devices rather than only this one's row.
#[tauri::command]
pub fn save_baby(state: State<'_, AppState>, baby: Baby) -> CmdResult<Baby> {
    // Carry the picture in the event, like an entry's photo: a peer has no
    // access to this device's photos/ directory. Read it before the repo lock.
    let photo = match (baby.id.as_deref(), baby.photo.as_deref()) {
        (Some(id), Some(_)) => {
            photo::read_photo(&state.photos_dir(), &photo::baby_photo_id(id)).ok()
        }
        _ => None,
    };

    let repo = state.repo();
    let key = repo.device_key().map_err(fail)?;
    let mut clock = repo.clock(key.id()).map_err(fail)?;
    repo.save_profile_with_photo(&key, &mut clock, &baby, photo.as_deref())
        .map_err(fail)
}

/// Point this device at a child. Local only: it changes nobody else's view.
#[tauri::command]
pub fn set_current_baby(state: State<'_, AppState>, baby_id: String) -> CmdResult<()> {
    state.repo().set_current_baby(&baby_id).map_err(fail)
}

#[tauri::command]
pub fn list_caregivers(state: State<'_, AppState>) -> CmdResult<Vec<Caregiver>> {
    state.repo().caregivers().map_err(fail)
}

/// A ready-to-send invite link for the child this device is looking at.
///
/// The link carries the topic, the group key, the role and *this device's
/// address* (see `little_log_core::invite`), so it is a secret: whoever holds it
/// can read and add to that child's log. It needs no server — the other device
/// redeems it and dials us directly.
#[tauri::command]
pub fn create_invite(state: State<'_, AppState>, role: Role) -> CmdResult<String> {
    eprintln!(
        "cmd: create_invite start (thread={:?})",
        std::thread::current().name()
    );
    let Some(sync) = state.sync() else {
        return Err(
            "Peer sync is not running on this device, so there is no address to invite to."
                .to_string(),
        );
    };
    let result = invite::Invite::mint(&state.repo(), role, sync.ticket())
        .map(|invite| invite.encode())
        .map_err(fail);
    eprintln!("cmd: create_invite done");
    result
}

/// Where a join request stands, and something to show the person waiting.
#[derive(Serialize)]
pub struct JoinProgress {
    pub state: JoinState,
    pub message: String,
}

/// Ask to join the child an invite link is for.
///
/// Returns at once: whether this device is let in is someone else's decision, so
/// the UI polls [`poll_join`]. The link carries no key, so all this buys is a
/// place in the queue — which is the point of the accept phase.
#[tauri::command]
pub fn request_join(state: State<'_, AppState>, link: String) -> CmdResult<()> {
    let invite = invite::Invite::decode(&link).map_err(fail)?;
    invite.remember(&state.repo()).map_err(fail)?;
    Ok(())
}

/// Where the join request stands, asked of the inviter.
///
/// Polling is the whole mechanism: the host answers "pending" until a person
/// there accepts, and the `accepted` answer is the only thing that carries the
/// key. On that answer this takes the key and pulls the history.
#[tauri::command]
pub fn poll_join(state: State<'_, AppState>, link: String) -> CmdResult<JoinProgress> {
    eprintln!(
        "cmd: poll_join start (thread={:?})",
        std::thread::current().name()
    );
    let invite = invite::Invite::decode(&link).map_err(fail)?;
    let Some(sync) = state.sync() else {
        return Err("Peer sync is not running on this device, so it cannot ask.".to_string());
    };
    let device = state.repo().device_key().map_err(fail)?.id();

    let reply = sync.ask_to_join(&invite, &device).map_err(fail)?;
    let progress = match reply.state {
        JoinState::Pending => JoinProgress {
            state: JoinState::Pending,
            message: "Waiting to be accepted…".to_string(),
        },
        JoinState::Declined => JoinProgress {
            state: JoinState::Declined,
            message: format!("{} declined this device.", invite.by),
        },
        JoinState::Accepted => {
            let key = reply
                .key
                .ok_or_else(|| "accepted, but no key came back".to_string())?;
            invite.accept_key(&state.repo(), &key).map_err(fail)?;

            // Now that the child is readable, pull the history — "joined" should
            // mean "and I can see them".
            let mut reached = true;
            if let Err(error) = sync.sync_with(&invite.addr) {
                reached = false;
                eprintln!("join: accepted but the first round failed: {error}");
            }
            let repo = state.repo();
            let name = repo
                .baby()
                .map_err(fail)?
                .map(|baby| baby.name)
                .unwrap_or_else(|| "the invited child".to_string());
            let entries = repo.entries().map_err(fail)?.len();
            JoinProgress {
                state: JoinState::Accepted,
                message: if reached {
                    format!("Joined {name} — {entries} entries so far.")
                } else {
                    format!("Joined {name}. The history will arrive when they are reachable.")
                },
            }
        }
    };
    eprintln!("cmd: poll_join done");
    Ok(progress)
}

/// Devices waiting to be let into the child this device is looking at.
#[tauri::command]
pub fn join_requests(state: State<'_, AppState>) -> CmdResult<Vec<JoinRequest>> {
    let repo = state.repo();
    let topic = repo
        .current_baby()
        .map_err(fail)?
        .and_then(|baby| baby.id)
        .ok_or_else(|| "there is no child to let anyone into".to_string())?;
    repo.join_requests(&topic).map_err(fail)
}

/// Accept or decline one of them.
///
/// Only an owner or co-parent may: admitting someone hands over the key that
/// opens the whole log, which is the same thing as inviting them.
#[tauri::command]
pub fn resolve_join(state: State<'_, AppState>, device: String, accept: bool) -> CmdResult<()> {
    let repo = state.repo();
    let role = repo
        .caregivers()
        .map_err(fail)?
        .first()
        .map(|caregiver| caregiver.role);
    if !matches!(role, Some(Role::Owner) | Some(Role::CoParent)) {
        return Err("Only an owner or co-parent can let someone in.".to_string());
    }
    let topic = repo
        .current_baby()
        .map_err(fail)?
        .and_then(|baby| baby.id)
        .ok_or_else(|| "there is no child to let anyone into".to_string())?;

    let state_wanted = if accept {
        JoinState::Accepted
    } else {
        JoinState::Declined
    };
    repo.resolve_join_request(&topic, &device, state_wanted)
        .map_err(fail)
}

/// The action a widget tap asked the app to open, if one is waiting.
///
/// Read once: the sheet opens on the tap that asked for it, not on every
/// resume. Nothing is logged here — saving that sheet writes the entry.
#[tauri::command]
pub fn take_widget_action(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    widget::take_pending_action(state.widget_paths()).map_err(fail)
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
///
/// Prefer the local file. A device that received the entry over sync has no
/// file, so fall back to the copy carried inside the entry's event.
#[tauri::command]
pub fn photo_data_url(state: State<'_, AppState>, entry_id: String) -> CmdResult<String> {
    let bytes = match photo::read_photo(&state.photos_dir(), &entry_id) {
        Ok(bytes) => bytes,
        Err(_) => state
            .repo()
            .photo_bytes(&entry_id)
            .map_err(fail)?
            .ok_or_else(|| "photo not found on this device".to_string())?,
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:image/jpeg;base64,{encoded}"))
}

/// Store a child's profile picture: downscaled to ≤1280 px JPEG.
/// Returns the file name to record on the profile.
#[tauri::command]
pub fn save_baby_photo(
    state: State<'_, AppState>,
    baby_id: String,
    bytes: Vec<u8>,
) -> CmdResult<String> {
    photo::write_photo(&state.photos_dir(), &photo::baby_photo_id(&baby_id), &bytes).map_err(fail)
}

/// The profile picture as a data URL, for the avatar in the WebView.
///
/// Prefer the local file. A device that received the profile over sync has no
/// file, so fall back to the copy carried inside the profile event.
#[tauri::command]
pub fn baby_photo_data_url(state: State<'_, AppState>, baby_id: String) -> CmdResult<String> {
    let name = photo::baby_photo_id(&baby_id);
    let bytes = match photo::read_photo(&state.photos_dir(), &name) {
        Ok(bytes) => bytes,
        Err(_) => state
            .repo()
            .baby_photo_bytes(&baby_id)
            .map_err(fail)?
            .ok_or_else(|| "photo not found on this device".to_string())?,
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:image/jpeg;base64,{encoded}"))
}

/* ------------------------------------------------------------------- sync */

/// Ask for a round with every peer this device knows, in the background.
///
/// Returns at once: a round is a catch-up, not something the UI should wait on,
/// and the screens refresh from the log either way.
#[tauri::command]
pub fn sync_now(state: State<'_, AppState>) -> CmdResult<SyncStatus> {
    match state.sync() {
        Some(sync) => {
            sync.sync_in_background();
            Ok(sync.snapshot())
        }
        None => Ok(SyncStatus::stopped(state.repo().peers().map_err(fail)?)),
    }
}

/// The current sync state and what each known peer last did.
#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> CmdResult<SyncStatus> {
    match state.sync() {
        Some(sync) => Ok(sync.snapshot()),
        None => Ok(SyncStatus::stopped(state.repo().peers().map_err(fail)?)),
    }
}
