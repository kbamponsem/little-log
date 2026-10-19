//! Peer sync: how two devices catch up with each other.
//!
//! There is no server, so replication is two devices talking over iroh. iroh
//! dials by public key rather than by address, which removes the two hardest
//! parts of a hand-rolled transport — NAT traversal and discovery — and its
//! endpoints are authenticated by a key, which is the identity this app already
//! has. See [`endpoint`] for why one ed25519 key serves both jobs.
//!
//! ## What is exchanged, and why that is enough
//!
//! The log is append-only and merging is set union, so catching up is one
//! question: *what do you have that I do not?* Each side answers with a version
//! vector (the newest event per authoring device, per topic) and the other
//! replies with the gap. Doing that in both directions leaves both devices
//! holding the same set, whatever order events arrive in.
//!
//! A new device is not a special case: it holds nothing, so its vector is empty
//! and the gap it is sent is the whole history. That is what joining is.

pub mod endpoint;
pub mod protocol;

pub use endpoint::{
    bind, decode_addr, dialable_addr, encode_addr, endpoint, join_once, listen, peer_id, serve,
    sync_once, Relays,
};
pub use protocol::{ask_to_join, serve_round, sync_round, JoinReply, SyncMessage, SyncReport};

/// Re-exported so a caller can hold an endpoint and its accept loop without
/// depending on iroh itself: this crate is the only place that should care which
/// transport is underneath.
pub use iroh::{endpoint::Connection, protocol::Router, Endpoint};
