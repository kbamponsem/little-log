//! Android's running-timer notifications, driven from Rust.
//!
//! The notification is a *view* of the open entries in SQLite: the Android
//! plugin never stores a start time or a running flag of its own. Every call
//! here carries the values derived by [`little_log_core::timer`], and the
//! Kotlin side only renders them.
//!
//! The plugin is registered on Android only. On desktop and iOS the helpers
//! below are no-ops, so callers do not need to branch on the platform.

#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use serde::Serialize;

use little_log_core::timer::{ActiveTimer, TimerKind};

#[cfg(not(target_os = "android"))]
use tauri::{plugin::TauriPlugin, Runtime};
#[cfg(target_os = "android")]
use tauri::{
    plugin::{PluginHandle, TauriPlugin},
    AppHandle, Manager, Runtime,
};

/// The payload the Kotlin plugin receives for one running timer.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerPayload {
    pub kind: &'static str,
    pub entry_id: String,
    pub baby_id: Option<String>,
    pub baby_name: String,
    pub started_at: i64,
    /// The side of a breast feed, so the notification can offer "Switch side".
    pub feed_kind: Option<&'static str>,
}

impl From<&ActiveTimer> for TimerPayload {
    fn from(timer: &ActiveTimer) -> Self {
        Self {
            kind: timer.kind.as_str(),
            entry_id: timer.entry_id.clone(),
            baby_id: timer.baby_id.clone(),
            baby_name: timer.baby_name.clone(),
            started_at: timer.started_at,
            feed_kind: timer.feed_kind.map(|kind| kind.as_str()),
        }
    }
}

#[derive(Serialize)]
struct SyncPayload {
    timers: Vec<TimerPayload>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StopPayload {
    kind: &'static str,
    baby_id: Option<String>,
    entry_id: Option<String>,
}

/// The registered Android plugin handle, kept in Tauri's managed state.
#[cfg(target_os = "android")]
pub struct TimerNotifications<R: Runtime>(PluginHandle<R>);

/// Register the Kotlin `app.littlelog.timer.TimerPlugin` class.
///
/// This is the only new plugin: it exists so the app can ask Android to show,
/// update or remove a timer without going through the WebView.
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    #[allow(unused_mut)]
    let mut builder = tauri::plugin::Builder::new("timer-notification");
    #[cfg(target_os = "android")]
    {
        builder = builder.setup(|app, api| {
            let handle = api.register_android_plugin("app.littlelog.timer", "TimerPlugin")?;
            app.manage(TimerNotifications(handle));
            Ok(())
        });
    }
    builder.build()
}

/// A new timer started. The notification id is derived from the child and kind,
/// so starting another feed for the same child replaces the first.
#[cfg(target_os = "android")]
pub fn start<R: Runtime>(app: &AppHandle<R>, timer: &ActiveTimer) -> Result<(), String> {
    send(app, "startTimer", &TimerPayload::from(timer))
}

/// An existing timer was edited (for example a different start time or side).
#[cfg(target_os = "android")]
pub fn update<R: Runtime>(app: &AppHandle<R>, timer: &ActiveTimer) -> Result<(), String> {
    send(app, "updateTimer", &TimerPayload::from(timer))
}

/// The timer stopped, was deleted, or sync replaced it with an ended entry.
#[cfg(target_os = "android")]
pub fn stop<R: Runtime>(
    app: &AppHandle<R>,
    kind: TimerKind,
    baby_id: Option<&str>,
    entry_id: Option<&str>,
) -> Result<(), String> {
    send(
        app,
        "stopTimer",
        &StopPayload {
            kind: kind.as_str(),
            baby_id: baby_id.map(str::to_string),
            entry_id: entry_id.map(str::to_string),
        },
    )
}

/// Replace the whole set of notifications with the timers that are open now.
///
/// Used on launch and after a sync round, where the app cannot know which peer
/// change started or stopped something. The Kotlin side cancels any of its own
/// timer notifications that are not in the list, so a stop from another device
/// removes the lock-screen timer too.
#[cfg(target_os = "android")]
pub fn sync<R: Runtime>(app: &AppHandle<R>, timers: &[ActiveTimer]) -> Result<(), String> {
    let payload = SyncPayload {
        timers: timers.iter().map(TimerPayload::from).collect(),
    };
    send(app, "syncTimers", &payload)
}

#[cfg(target_os = "android")]
fn send<R: Runtime, T: Serialize>(
    app: &AppHandle<R>,
    command: &str,
    payload: &T,
) -> Result<(), String> {
    let Some(plugin) = app.try_state::<TimerNotifications<R>>() else {
        // The plugin setup failed or this is not a mobile build; a missing
        // notification must never break logging.
        return Ok(());
    };
    plugin
        .0
        .run_mobile_plugin::<serde_json::Value>(command, payload)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "android"))]
pub fn start<R: Runtime>(_: &tauri::AppHandle<R>, _: &ActiveTimer) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "android"))]
pub fn update<R: Runtime>(_: &tauri::AppHandle<R>, _: &ActiveTimer) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "android"))]
pub fn stop<R: Runtime>(
    _: &tauri::AppHandle<R>,
    _: TimerKind,
    _: Option<&str>,
    _: Option<&str>,
) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "android"))]
pub fn sync<R: Runtime>(_: &tauri::AppHandle<R>, _: &[ActiveTimer]) -> Result<(), String> {
    Ok(())
}
