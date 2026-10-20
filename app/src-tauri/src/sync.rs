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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use little_log_core::validate::now_millis;
use little_log_core::{CoreError, Repo};
use little_log_sync::{
    bind, decode_addr, dialable_addr, encode_addr, join_once, serve, sync_once, Endpoint, Relays,
    Router,
};
use serde::Serialize;
use tokio::runtime::Runtime;

/// How one peer's last attempt went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerSyncState {
    #[default]
    Idle,
    Syncing,
    Ok,
    Error,
}

/// One device this log knows how to reach.
#[derive(Clone, Debug, Serialize)]
pub struct PeerSyncStatus {
    /// The stored ticket, so an undecodable address can still be shown.
    pub addr: String,
    /// The peer's device id, when the ticket decodes.
    pub peer_id: Option<String>,
    pub state: PeerSyncState,
    pub last_synced_at: Option<i64>,
    /// What this peer moved in the last completed round.
    pub received: usize,
    pub sent: usize,
    pub error: Option<String>,
}

impl PeerSyncStatus {
    fn idle(addr: String) -> Self {
        let peer_id = decode_addr(&addr).ok().map(|addr| addr.id.to_string());
        Self {
            addr,
            peer_id,
            state: PeerSyncState::Idle,
            last_synced_at: None,
            received: 0,
            sent: 0,
            error: None,
        }
    }
}

/// What the Settings sync card shows.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SyncStatus {
    pub running: bool,
    pub last_synced_at: Option<i64>,
    /// Totals for the last completed round.
    pub received: usize,
    pub sent: usize,
    pub peers: Vec<PeerSyncStatus>,
    pub error: Option<String>,
}

impl SyncStatus {
    /// No endpoint on this device: still show the peers we know about.
    pub fn stopped(addrs: Vec<String>) -> Self {
        Self {
            running: false,
            last_synced_at: None,
            received: 0,
            sent: 0,
            peers: addrs.into_iter().map(PeerSyncStatus::idle).collect(),
            error: Some("Peer sync is not running on this device.".to_string()),
        }
    }
}

/// The app's peer sync, or the reason it is not running.
pub struct SyncService {
    runtime: Runtime,
    endpoint: Endpoint,
    database: PathBuf,
    /// Shared with the background rounds and read by the Settings screen.
    status: Arc<Mutex<SyncStatus>>,
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
            status: Arc::new(Mutex::new(SyncStatus::default())),
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
        // Tell the host where to reach us too, so its peer list is not empty
        // and either side can start the next round.
        let self_addr = encode_addr(&self.endpoint.addr());
        let reply = self.runtime.block_on(async move {
            join_once(&endpoint, addr, &invite, &device, &self_addr).await
        });
        eprintln!("sync: ask_to_join done");
        reply
    }

    /// A snapshot for the UI. Peers come from the database as well, so a newly
    /// remembered invite shows up before the first round touches it.
    pub fn snapshot(&self) -> SyncStatus {
        let mut status = self.lock_status().clone();
        if let Ok(repo) = Repo::open(&self.database) {
            if let Ok(addrs) = repo.peers() {
                for addr in addrs {
                    if !status.peers.iter().any(|peer| peer.addr == addr) {
                        status.peers.push(PeerSyncStatus::idle(addr));
                    }
                }
            }
        }
        status
            .peers
            .sort_by(|a, b| a.peer_id.cmp(&b.peer_id).then_with(|| a.addr.cmp(&b.addr)));
        status
    }

    fn lock_status(&self) -> std::sync::MutexGuard<'_, SyncStatus> {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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
                let mut status = self.lock_status();
                status.running = false;
                status.error = Some(error.to_string());
                return;
            }
        };

        {
            let mut status = self.lock_status();
            if status.running {
                return;
            }
            status.running = true;
            status.received = 0;
            status.sent = 0;
            status.error = None;
            status.peers = peers.iter().cloned().map(PeerSyncStatus::idle).collect();
            for peer in &mut status.peers {
                peer.state = PeerSyncState::Syncing;
            }
        }

        if peers.is_empty() {
            let mut status = self.lock_status();
            status.running = false;
            status.last_synced_at = Some(now_millis());
            return;
        }

        let pending = Arc::new(AtomicUsize::new(peers.len()));
        for addr in peers {
            let database = database.clone();
            let endpoint = endpoint.clone();
            let status = self.status.clone();
            let pending = pending.clone();

            self.runtime.spawn(async move {
                let outcome: Result<little_log_sync::SyncReport, String> = async {
                    let mut repo = Repo::open(&database).map_err(|error| error.to_string())?;
                    let topics = repo.topics().map_err(|error| error.to_string())?;
                    let addr = decode_addr(&addr).map_err(|error| error.to_string())?;
                    sync_once(&endpoint, addr, &mut repo, &topics)
                        .await
                        .map_err(|error| error.to_string())
                }
                .await;

                let now = now_millis();
                {
                    let mut status = status
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if let Some(peer) = status.peers.iter_mut().find(|peer| peer.addr == addr) {
                        match &outcome {
                            Ok(report) => {
                                peer.state = PeerSyncState::Ok;
                                peer.last_synced_at = Some(now);
                                peer.received = report.received;
                                peer.sent = report.sent;
                                peer.error = None;
                                status.received += report.received;
                                status.sent += report.sent;
                            }
                            Err(error) => {
                                peer.state = PeerSyncState::Error;
                                peer.error = Some(error.clone());
                            }
                        }
                    }
                }

                // The last peer to finish closes the round, so the UI can stop
                // showing a spinner even when one peer was unreachable.
                if pending.fetch_sub(1, Ordering::SeqCst) == 1 {
                    let mut status = status
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    status.running = false;
                    status.last_synced_at = Some(now);
                    let failed = status
                        .peers
                        .iter()
                        .filter(|peer| peer.state == PeerSyncState::Error)
                        .count();
                    status.error = (failed > 0).then(|| {
                        if failed == 1 {
                            "One peer could not be reached.".to_string()
                        } else {
                            format!("{failed} peers could not be reached.")
                        }
                    });
                }
            });
        }
    }
}
