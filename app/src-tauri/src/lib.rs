//! Little Log — the app shell.
//!
//! The frontend (`../web`, TypeScript web components) talks to these commands
//! through Tauri's IPC. Every command is a thin adapter over `little-log-core`,
//! so the app, the Android widget and the sync server share one schema, one
//! validation pass and one last-write-wins rule.

mod commands;
mod settings;
mod widget;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use little_log_core::model::{Caregiver, Role};
use little_log_core::Repo;
use tauri::Manager;

pub use commands::*;
pub use settings::Settings;
pub use widget::WidgetPaths;

/// Shared state: the on-device database, the widget's files and the app-local
/// settings file.
pub struct AppState {
    repo: Mutex<Repo>,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    paths: WidgetPaths,
}

impl AppState {
    /// Open the database, make sure the folders exist, drain any widget taps
    /// that arrived while the app was closed, and create the local profile on
    /// first run.
    pub fn initialise(app: &tauri::AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = app.path().app_data_dir()?;
        let paths = WidgetPaths::new(data_dir);
        std::fs::create_dir_all(&paths.data_dir)?;
        // Tell the widget worker which directory the app actually uses.
        paths.publish_root()?;
        std::fs::create_dir_all(paths.photos())?;

        let settings_path = paths.data_dir.join("settings.json");
        let settings = Settings::load(&settings_path);

        let repo = Repo::open(paths.database())?;
        let state = Self {
            repo: Mutex::new(repo),
            settings: Mutex::new(settings),
            settings_path,
            paths,
        };
        state.bootstrap()?;

        // One-tap actions queued while the app was closed (the widget can't
        // open the app, so it leaves them for us).
        let _ = widget::drain_and_refresh(&state.paths, widget::tz_offset_minutes());
        Ok(state)
    }

    /// Lock helpers: a panicking command must not wedge the whole app.
    pub fn repo(&self) -> MutexGuard<'_, Repo> {
        self.repo
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn store_settings(&self, next: Settings) {
        {
            let mut settings = self
                .settings
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *settings = next.clone();
        }
        let _ = next.save(&self.settings_path);
    }

    pub fn photos_dir(&self) -> PathBuf {
        self.paths.photos()
    }

    pub fn widget_paths(&self) -> &WidgetPaths {
        &self.paths
    }

    /// The app is local-first: a baby profile and a local owner exist before
    /// anyone signs in to anything.
    fn bootstrap(&self) -> Result<(), little_log_core::CoreError> {
        let repo = self.repo();
        let baby = repo.ensure_baby("Baby")?;
        if repo.count_caregivers()? == 0 {
            let mut owner = Caregiver::new("You", Role::Owner);
            owner.baby_id = baby.id;
            repo.save_caregiver(&owner)?;
        }
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = AppState::initialise(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::device_identity,
            commands::list_entries,
            commands::save_entry,
            commands::delete_entry,
            commands::get_baby,
            commands::save_baby,
            commands::list_caregivers,
            commands::create_invite,
            commands::export_csv,
            commands::delete_all,
            commands::save_photo,
            commands::photo_data_url,
            commands::sync_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Little Log");
}
