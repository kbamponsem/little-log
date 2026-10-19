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

use std::sync::{Arc, Mutex, MutexGuard};

use little_log_core::model::{Caregiver, Role};
use little_log_core::Repo;
use tauri::Manager;

pub use commands::*;
pub use settings::Settings;
pub mod sync;
pub use widget::WidgetPaths;

/// Shared state: the on-device database, the widget's files and the app-local
/// settings file.
pub struct AppState {
    repo: Mutex<Repo>,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    paths: WidgetPaths,
    /// Peer sync, or `None` when it could not start. A log with no network is
    /// still a working log, so this is not allowed to be fatal.
    sync: Option<Arc<sync::SyncService>>,
}

impl AppState {
    /// Open the database, make sure the folders exist, drain any widget taps
    /// that arrived while the app was closed, and create the local profile on
    /// first run.
    pub fn initialise(app: &tauri::AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        eprintln!("init: app_data_dir start");
        let data_dir = app.path().app_data_dir()?;
        eprintln!("init: app_data_dir done");
        let paths = WidgetPaths::new(data_dir);
        eprintln!("init: create_dir_all start");
        std::fs::create_dir_all(&paths.data_dir)?;
        eprintln!("init: create_dir_all done");
        // Tell the widget worker which directory the app actually uses.
        eprintln!("init: publish_root start");
        paths.publish_root()?;
        eprintln!("init: publish_root done");
        eprintln!("init: photos_dir start");
        std::fs::create_dir_all(paths.photos())?;
        eprintln!("init: photos_dir done");

        let settings_path = paths.data_dir.join("settings.json");
        eprintln!("init: Settings::load start");
        let settings = Settings::load(&settings_path);
        eprintln!("init: Settings::load done");

        eprintln!("init: Repo::open start");
        let t = std::time::Instant::now();
        let repo = Repo::open(paths.database())?;
        eprintln!("init: Repo::open done {:?}", t.elapsed());

        // Peer sync listens on the device key, so it starts from the same secret
        // the log signs with. A failure here (no permission, no network) must not
        // stop the app opening.
        eprintln!("init: device_key start");
        let sync = match repo.device_key() {
            Ok(key) => {
                eprintln!("init: device_key done");
                eprintln!("init: SyncService::start start");
                let t = std::time::Instant::now();
                match sync::SyncService::start(key.secret_bytes(), paths.database()) {
                    Ok(service) => {
                        eprintln!("init: SyncService::start done {:?}", t.elapsed());
                        Some(Arc::new(service))
                    }
                    Err(error) => {
                        eprintln!("sync: not started: {error}");
                        None
                    }
                }
            }
            Err(error) => {
                eprintln!("sync: no device key: {error}");
                None
            }
        };

        let state = Self {
            repo: Mutex::new(repo),
            settings: Mutex::new(settings),
            settings_path,
            paths,
            sync,
        };
        eprintln!("init: bootstrap start");
        let t = std::time::Instant::now();
        state.bootstrap()?;
        eprintln!("init: bootstrap done {:?}", t.elapsed());

        // One-tap actions queued while the app was closed (the widget can't
        // open the app, so it leaves them for us).
        eprintln!("init: drain_and_refresh start");
        let _ = widget::drain_and_refresh(&state.paths, widget::tz_offset_minutes());
        eprintln!("init: drain_and_refresh done");

        // Catch up with any peer we know about. Opening the app is exactly when a
        // parent expects yesterday's feeds from the other phone to appear.
        if let Some(sync) = &state.sync {
            eprintln!("init: sync_in_background start");
            sync.sync_in_background();
            eprintln!("init: sync_in_background done");
        }
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

    /// The sync service, when it started.
    pub fn sync(&self) -> Option<&Arc<sync::SyncService>> {
        self.sync.as_ref()
    }

    /// Ask for a background round with every known peer, if sync is running.
    pub fn sync_in_background(&self) {
        if let Some(sync) = &self.sync {
            sync.sync_in_background();
        }
    }

    /// The app is local-first: a local owner exists before anyone signs in to
    /// anything.
    ///
    /// No child is invented here. `ensure_baby` used to run at startup, which
    /// meant a log already had a baby called "Baby" by the time the UI could
    /// ask for one — so an intro screen was impossible. The owner is linked to
    /// a child only if one already exists.
    fn bootstrap(&self) -> Result<(), little_log_core::CoreError> {
        let repo = self.repo();
        if repo.count_caregivers()? == 0 {
            let mut owner = Caregiver::new("You", Role::Owner);
            owner.baby_id = repo.baby()?.and_then(|baby| baby.id);
            repo.save_caregiver(&owner)?;
        }
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    eprintln!("boot: run() entered");
    // reqwest comes in through iroh with `rustls-no-provider`, so a provider
    // must be installed before anything (the sync endpoint in `setup`) builds a
    // client. ring is what the Android build uses; see Cargo.toml.
    let _ = rustls::crypto::ring::default_provider().install_default();
    eprintln!("boot: rustls provider installed");

    tauri::Builder::default()
        .setup(|app| {
            eprintln!("boot: setup start");
            let state = AppState::initialise(app.handle())?;
            app.manage(state);
            eprintln!("boot: setup done");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::device_identity,
            commands::device_theme,
            commands::save_device_theme,
            commands::list_entries,
            commands::save_entry,
            commands::delete_entry,
            commands::list_babies,
            commands::current_baby,
            commands::create_baby,
            commands::save_baby,
            commands::set_current_baby,
            commands::list_caregivers,
            commands::create_invite,
            commands::request_join,
            commands::poll_join,
            commands::join_requests,
            commands::resolve_join,
            commands::export_csv,
            commands::delete_all,
            commands::save_photo,
            commands::photo_data_url,
            commands::take_widget_action,
            commands::sync_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Little Log");
}
