//! The app's sync service: one endpoint, one runtime, rounds on demand.
//!
//! There is no server and no background daemon: this device listens for peers
//! while the app is open, and dials the peers it has been told about (from the
//! invites it redeemed) whenever the app opens or something is logged. That is
//! enough for two parents on the same Wi-Fi, and a round is idempotent, so
//! syncing more often than necessary costs nothing but a little traffic.
//!
//! Failure is never fatal: a device with no network is still a working log, so
//! every entry point here degrades to "did nothing" rather than an error the UI
//! has to handle.

use std::path::PathBuf;
use std::sync::Arc;

use little_log_core::{CoreError, Repo};
use little_log_sync::{
    bind, decode_addr, dialable_addr, encode_addr, join_once, serve, sync_once, Endpoint, Relays,
    Router,
};
use tokio::runtime::Runtime;

/// The app's peer sync, or the reason it is not running.
pub struct SyncService {
    runtime: Runtime,
    endpoint: Endpoint,
    database: PathBuf,
    /// Held so the accept loop lives as long as the service does.
    _router: Router,
}

impl SyncService {
    /// Bind this device's endpoint and start accepting peers.
    ///
    /// `secret` is the device key: the same key that signs events, handed to
    /// iroh so a peer's identity *is* the identity on its events.
    pub fn start(secret: [u8; 32], database: PathBuf) -> Result<Self, CoreError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| CoreError::data(format!("could not start sync runtime: {error}")))?;
        eprintln!("sync: runtime built");

        // Both inside the runtime: `serve` starts the accept loop with
        // `tokio::task::spawn`, and Tauri's setup thread has no runtime in
        // context, so spawning there aborts the process.
        eprintln!("sync: binding endpoint");
        let (endpoint, router) = runtime.block_on(async {
            let endpoint = bind(secret, Relays::Enabled).await?;
            eprintln!("sync: bound");
            let router = serve(endpoint.clone(), database.clone());
            eprintln!("sync: router serving");
            Ok::<_, little_log_core::CoreError>((endpoint, router))
        })?;

        Ok(Self {
            runtime,
            endpoint,
            database,
            _router: router,
        })
    }

    /// This device's address, as an invite carries it.
    ///
    /// Waits for a relay connection first: a ticket minted before the endpoint
    /// is online can lack a relay URL and be unreachable from another network.
    pub fn ticket(&self) -> String {
        eprintln!("sync: ticket start");
        let ticket = self
            .runtime
            .block_on(async { encode_addr(&dialable_addr(&self.endpoint).await) });
        eprintln!("sync: ticket done");
        ticket
    }

    /// This device's peer id, for diagnostics.
    pub fn peer_id(&self) -> String {
        little_log_sync::peer_id(&self.endpoint)
    }

    /// A round with one peer, waited for. Used when redeeming an invite, where
    /// the caller wants to know whether the history actually arrived.
    pub fn sync_with(&self, addr: &str) -> Result<little_log_sync::SyncReport, CoreError> {
        eprintln!("sync: sync_with start");
        let addr = decode_addr(addr)?;
        let database = self.database.clone();
        let endpoint = self.endpoint.clone();
        let report = self.runtime.block_on(async move {
            let mut repo = Repo::open(&database)?;
            let topics = repo.topics()?;
            sync_once(&endpoint, addr, &mut repo, &topics).await
        });
        eprintln!("sync: sync_with done");
        report
    }

    /// Ask the inviter whether this device may join, waited for.
    ///
    /// One ask, not a wait: somebody on the other side has to decide, so the
    /// answer is usually "still waiting" and the caller asks again.
    pub fn ask_to_join(
        &self,
        invite: &little_log_core::invite::Invite,
        device: &str,
    ) -> Result<little_log_sync::JoinReply, CoreError> {
        eprintln!("sync: ask_to_join start");
        let addr = decode_addr(&invite.addr)?;
        let endpoint = self.endpoint.clone();
        let invite = invite.clone();
        let device = device.to_string();
        let reply = self
            .runtime
            .block_on(async move { join_once(&endpoint, addr, &invite, &device).await });
        eprintln!("sync: ask_to_join done");
        reply
    }

    /// A round with every peer this device knows, in the background.
    ///
    /// Deliberately not awaited: a slow or absent peer must not hold up logging
    /// a nappy. Each round converges whatever it can, and the next one re-asks.
    pub fn sync_in_background(self: &Arc<Self>) {
        let database = self.database.clone();
        let endpoint = self.endpoint.clone();
        let peers = match Repo::open(&database).and_then(|repo| repo.peers()) {
            Ok(peers) => peers,
            Err(error) => {
                eprintln!("sync: could not list peers: {error}");
                return;
            }
        };

        for addr in peers {
            let database = database.clone();
            let endpoint = endpoint.clone();
            self.runtime.spawn(async move {
                let Ok(mut repo) = Repo::open(&database) else {
                    return;
                };
                let Ok(topics) = repo.topics() else { return };
                let Ok(addr) = decode_addr(&addr) else { return };
                match sync_once(&endpoint, addr, &mut repo, &topics).await {
                    Ok(report) if report.received > 0 || report.sent > 0 => {
                        eprintln!("sync: moved {} in, {} out", report.received, report.sent);
                    }
                    Ok(_) => {}
                    Err(error) => eprintln!("sync: round failed: {error}"),
                }
            });
        }
    }
}
