//! A peer device, for trying a join without a second phone.
//!
//! Creates its own log and child, prints an invite link, then listens and keeps
//! syncing with whoever redeems it. Point the app's *Join with a code* sheet at
//! the printed link and the two logs should converge.
//!
//! ```text
//! cargo run -p little-log-sync --example peer -- /tmp/little-log-peer
//! ```

use std::path::PathBuf;
use std::time::Duration;

use little_log_core::invite::Invite;
use little_log_core::model::{Entry, EntryKind, Role};
use little_log_core::Repo;
use little_log_sync::{bind, decode_addr, encode_addr, serve, sync_once, Relays};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/little-log-peer".to_string())
        .into();
    std::fs::create_dir_all(&directory)?;
    let database = directory.join("peer.sqlite3");

    let mut repo = Repo::open(&database)?;
    let key = repo.device_key()?;
    let mut clock = repo.clock(key.id())?;

    // A child of its own, so there is a history worth joining.
    let baby = match repo.babies()?.first() {
        Some(baby) => baby.clone(),
        None => {
            let baby = repo.create_baby(&key, &mut clock, "Peer Baby", "2026-01-01")?;
            for (at, note) in [
                (1_760_000_000_000, "peer feed one"),
                (1_760_000_600_000, "peer feed two"),
            ] {
                let mut entry = Entry::new(EntryKind::Feed, at);
                entry.baby_id = baby.id.clone();
                entry.note = Some(note.to_string());
                repo.record_new_entry(&key, &mut clock, &entry)?;
            }
            baby
        }
    };

    // Direct-only: two machines on one network need no relay, and this way the
    // example is not waiting on anyone else's infrastructure.
    let endpoint = bind(key.secret_bytes(), Relays::Disabled).await?;
    let link = Invite::mint(&repo, Role::CoParent, encode_addr(&endpoint.addr()))?.encode();

    println!("peer id:     {}", little_log_sync::peer_id(&endpoint));
    println!("child:       {}", baby.name);
    println!("entries:     {}", repo.entries()?.len());
    println!("events:      {}", repo.log().count()?);
    println!();
    println!("invite link (paste this into the app's “Join with a code”):");
    println!("{link}");
    println!();

    let _router = serve(endpoint.clone(), database);

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let topics = repo.topics()?;
        let peers = repo.peers()?;
        for addr in peers {
            let Ok(addr) = decode_addr(&addr) else {
                continue;
            };
            match sync_once(&endpoint, addr, &mut repo, &topics).await {
                Ok(report) if report.received > 0 || report.sent > 0 => {
                    println!(
                        "round: {} in, {} out — log now {} event(s), {} entry(s)",
                        report.received,
                        report.sent,
                        repo.log().count()?,
                        repo.entries()?.len()
                    );
                }
                Ok(_) => {}
                Err(error) => println!("round failed: {error}"),
            }
        }
    }
}
