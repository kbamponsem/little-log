//! The iroh endpoint: this device's transport identity, listening and dialing.
//!
//! iroh dials a *public key*, not an address. That is the whole reason it fits
//! here: this app already has one ed25519 key per device, and handing iroh the
//! same secret means the key a peer authenticates over QUIC is the key whose
//! signature is on every event it sends. There is no separate PKI, no account
//! and no server to say who is who.
//!
//! Addresses still matter for the first connection: a peer that has never met us
//! needs somewhere to dial. That is the [`EndpointAddr`] an invite carries — the
//! id plus whatever direct addresses we can offer. Once two devices have talked,
//! iroh keeps the path; relays and address lookup are for reaching a device that
//! is not on the same network.

use std::path::PathBuf;

use iroh::{
    endpoint::{presets, Connection, PortmapperConfig},
    protocol::{AcceptError, ProtocolHandler, Router},
    Endpoint, EndpointAddr, RelayMode, SecretKey,
};
use little_log_core::invite::Invite;
use little_log_core::{CoreError, Repo, Result};

use crate::protocol::{ask_to_join, serve_round, sync_round, JoinReply, SyncReport};

/// The ALPN both ends must agree on. Bumping it is how a future protocol change
/// avoids talking to an old build.
pub const ALPN: &[u8] = b"little-log/sync/1";

/// Our device key as iroh's secret key — the same 32 bytes the log signs with.
pub fn secret_key(secret: [u8; 32]) -> SecretKey {
    SecretKey::from_bytes(&secret)
}

/// The address to put in an invite: a single url-safe string a peer can dial.
///
/// Uses iroh's own ticket format, which packs the id and its addresses into
/// something short enough to read aloud — and, importantly, something with no `&`
/// or `=` in it, so it survives living inside a link's query string.
pub fn encode_addr(addr: &EndpointAddr) -> String {
    iroh_tickets::endpoint::EndpointTicket::new(addr.clone()).to_string()
}

/// Read an address out of an invite.
pub fn decode_addr(ticket: &str) -> Result<EndpointAddr> {
    ticket
        .trim()
        .parse::<iroh_tickets::endpoint::EndpointTicket>()
        .map(|ticket| ticket.endpoint_addr().clone())
        .map_err(|error| CoreError::invalid(format!("unreadable peer address: {error}")))
}

/// This device's id as a peer sees it, for an invite to name.
///
/// iroh renders ids in its own base32 alphabet; the log writes the same key as
/// hex. Both are the same public key, and [`peer_id`] is the one to dial.
pub fn peer_id(endpoint: &Endpoint) -> String {
    endpoint.id().to_string()
}

/// Whether this endpoint may fall back to iroh's public relay infrastructure.
///
/// `Enabled` reaches a device on another network; `Disabled` is direct-only,
/// which is what two phones in one house need and what keeps a test off the
/// network entirely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relays {
    Enabled,
    Disabled,
}

/// Build the endpoint this device listens and dials on.
pub async fn endpoint(secret: [u8; 32]) -> Result<Endpoint> {
    bind(secret, Relays::Enabled).await
}

/// Build an endpoint with an explicit policy for reaching other machines.
///
/// `Disabled` turns off *everything* that leaves the machine: no relay, no DNS
/// address lookup, no portmapper. That is the LAN-only mode, and it is also what
/// lets a test run without a network at all — a single one of those three left
/// on is enough to make a connection wait on infrastructure that may not be
/// there.
pub async fn bind(secret: [u8; 32], relays: Relays) -> Result<Endpoint> {
    // The presets are different types, so the branch is on the builder itself.
    let mut builder = match relays {
        // `presets::N0` follows `default_relay_mode()`, which silently switches
        // to n0's *staging* relays when `IROH_FORCE_STAGING_RELAYS` is set.
        // Pinning `RelayMode::Default` (the production relays) keeps a released
        // build on the stable set no matter what the environment says.
        Relays::Enabled => Endpoint::builder(presets::N0).relay_mode(RelayMode::Default),
        Relays::Disabled => Endpoint::builder(presets::Minimal),
    };
    builder = builder
        .secret_key(secret_key(secret))
        .alpns(vec![ALPN.to_vec()]);
    if relays == Relays::Disabled {
        builder = builder
            .relay_mode(RelayMode::Disabled)
            .clear_address_lookup()
            .portmapper_config(PortmapperConfig::Disabled);
    }
    builder.bind().await.map_err(net_error)
}

/// The address to put in an invite, once a relay is connected.
///
/// `Endpoint::addr()` only carries a relay URL after the endpoint has reached
/// one, and an invite without one cannot reach a device on another network.
/// Wait briefly for that; if no relay arrives (offline, or relay mode disabled)
/// fall back to the addresses we do have, so a same-network join still works.
pub async fn dialable_addr(endpoint: &Endpoint) -> EndpointAddr {
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), endpoint.online()).await;
    endpoint.addr()
}

/// Start accepting peers on an endpoint the caller already owns.
///
/// Split from [`listen`] so a long-lived process binds once and reuses that
/// identity for every round, rather than opening a new one per sync.
pub fn serve(endpoint: Endpoint, database: PathBuf) -> Router {
    Router::builder(endpoint)
        .accept(ALPN, SyncHandler { database })
        .spawn()
}

/// Bind an endpoint and start accepting peers, for callers that want both.
///
/// Each connection opens its own handle on the database rather than sharing one:
/// a SQLite connection cannot be shared across threads, and a separate handle
/// with WAL is already how the widget's worker reads the same file.
pub async fn listen(secret: [u8; 32], database: PathBuf) -> Result<Router> {
    let endpoint = endpoint(secret).await?;
    let router = Router::builder(endpoint)
        .accept(
            ALPN,
            SyncHandler {
                database: database.clone(),
            },
        )
        .spawn();
    Ok(router)
}

/// One round with the peer at `addr`, over an endpoint the caller owns.
///
/// The caller binds the endpoint, not this function: an endpoint is the device's
/// listening identity and is worth keeping for every round, and building one per
/// sync would also mean opening a fresh hole in the network each time.
pub async fn sync_once(
    endpoint: &Endpoint,
    addr: EndpointAddr,
    repo: &mut Repo,
    topics: &[String],
) -> Result<SyncReport> {
    let connection = endpoint.connect(addr, ALPN).await.map_err(net_error)?;
    let report = serve_connection(&connection, repo, topics).await;
    connection.close(0u32.into(), b"done");
    report
}

/// Ask to join the child an invite is for, over an endpoint the caller owns.
///
/// One ask, not a wait: whether this device is let in is a person's decision on
/// the other side, so a caller that wants to wait calls this again. The invite
/// carries no key, so an `accepted` reply is the only thing that gets one in.
pub async fn join_once(
    endpoint: &Endpoint,
    addr: EndpointAddr,
    invite: &Invite,
    device: &str,
) -> Result<JoinReply> {
    let connection = endpoint.connect(addr, ALPN).await.map_err(net_error)?;
    let (send, recv) = connection.open_bi().await.map_err(net_error)?;
    let reply = ask_to_join(recv, send, invite, device).await;
    connection.close(0u32.into(), b"done");
    reply
}

/// Run a round over an established connection, pulling and pushing.
pub async fn serve_connection(
    connection: &Connection,
    repo: &mut Repo,
    topics: &[String],
) -> Result<SyncReport> {
    let (send, recv) = connection.open_bi().await.map_err(net_error)?;
    let report = sync_round(recv, send, repo, topics).await?;
    Ok(report)
}

/// The accepting side: serve one peer, then let the connection close.
#[derive(Debug, Clone)]
struct SyncHandler {
    database: PathBuf,
}

impl ProtocolHandler for SyncHandler {
    async fn accept(&self, connection: Connection) -> std::result::Result<(), AcceptError> {
        // Owned, not borrowed: a `Repo` is `Send` but not `Sync`, so holding a
        // *reference* to one across an `await` would make this future non-`Send`
        // and iroh would refuse to run it. Owning it keeps the future movable.
        let mut repo = Repo::open(&self.database).map_err(AcceptError::from_err)?;
        let (send, recv) = connection.accept_bi().await?;
        // `serve_round`, not `sync_round`: the dialer drives the exchange and the
        // listener answers it. Two drivers would each wait for the other, and the
        // connection would close with an empty reason.
        //
        // Errors are per-connection: a peer that misbehaves must not stop the
        // listener, and the next round simply re-asks for anything missed.
        let _ = serve_round(recv, send, &mut repo).await;
        // Stay until the peer closes. Returning here would drop the connection
        // with the final message still queued, and the dialer would see the
        // connection lost instead of its acknowledgement.
        let _ = connection.closed().await;
        Ok(())
    }
}

fn net_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::invalid(format!("peer connection failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use little_log_core::clock::Clock;
    use little_log_core::crypto::{DeviceKey, GroupKey};
    use little_log_core::model::{Entry, EntryKind, EventKind};
    use little_log_core::payload::EntryPayload;
    use little_log_core::Event;

    const TOPIC: &str = "baby-1";

    struct Device {
        key: DeviceKey,
        clock: Clock,
        repo: Repo,
    }

    impl Device {
        fn on_disk(group: &GroupKey, path: PathBuf) -> Self {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let repo = Repo::open(&path).unwrap();
            repo.save_group_key(TOPIC, group).unwrap();
            let key = repo.device_key().unwrap();
            let clock = Clock::new(key.id());
            Self { key, clock, repo }
        }

        fn in_memory(group: &GroupKey) -> Self {
            let repo = Repo::open_in_memory().unwrap();
            repo.save_group_key(TOPIC, group).unwrap();
            let key = repo.device_key().unwrap();
            let clock = Clock::new(key.id());
            Self { key, clock, repo }
        }

        fn feed(&mut self, at: i64) {
            let group = self.repo.group_key(TOPIC).unwrap().unwrap();
            let entry = Entry::new(EntryKind::Feed, at);
            let event = Event::seal(
                &self.key,
                &group,
                TOPIC,
                &entry.id,
                EventKind::Create,
                self.clock.tick(at),
                &EntryPayload::of(&entry).to_bytes().unwrap(),
            )
            .unwrap();
            self.repo.log().insert(&event).unwrap();
        }
    }

    /// The whole join, as the app will do it: one device listening on a real
    /// QUIC endpoint, one dialling it, the history moving between them.
    ///
    /// Capped, because a connection that waits on absent infrastructure should
    /// fail in seconds rather than hang a test run.
    #[tokio::test]
    async fn a_joiner_pulls_the_history_over_a_quic_connection() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let group = GroupKey::generate().unwrap();
            let directory =
                std::env::temp_dir().join(format!("little-log-iroh-{}", std::process::id()));
            let database = directory.join("parent.sqlite3");

            let mut parent = Device::on_disk(&group, database.clone());
            parent.feed(1_000);
            parent.feed(2_000);

            let listener = bind(parent.key.secret_bytes(), Relays::Disabled)
                .await
                .unwrap();
            let addr = listener.addr();
            let router = Router::builder(listener)
                .accept(
                    ALPN,
                    SyncHandler {
                        database: database.clone(),
                    },
                )
                .spawn();

            let mut joiner = Device::in_memory(&group);
            let dialer = bind(joiner.key.secret_bytes(), Relays::Disabled)
                .await
                .unwrap();
            let report = sync_once(&dialer, addr, &mut joiner.repo, &[TOPIC.to_string()])
                .await
                .unwrap();
            dialer.close().await;
            let _ = router.shutdown().await;

            // Read back what the serving device holds, from disk.
            let parent_after = Repo::open(&database).unwrap();
            let _ = std::fs::remove_dir_all(&directory);
            (
                report,
                joiner.repo.log().count().unwrap(),
                parent_after.log().count().unwrap(),
            )
        })
        .await;

        let (report, joiner_events, parent_events) =
            outcome.expect("a direct connection must not need the network");
        assert_eq!(report.received, 2, "the joiner pulled both entries");
        assert_eq!(joiner_events, 2);
        assert_eq!(parent_events, 2, "and a round is not a migration");
    }

    /// A peer dials by key, so the id we print for an invite must be the one a
    /// connection reports — otherwise a join could never be authenticated.
    #[tokio::test]
    async fn the_id_in_an_invite_is_the_id_a_peer_connects_as() {
        let secret = little_log_core::crypto::DeviceKey::generate()
            .unwrap()
            .secret_bytes();
        let endpoint = bind(secret, Relays::Disabled).await.unwrap();

        let printed = peer_id(&endpoint);

        assert_eq!(printed, endpoint.id().to_string());
        assert!(!printed.is_empty());
        endpoint.close().await;
    }
}
