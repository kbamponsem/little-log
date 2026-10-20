//! The anti-entropy exchange, over a pair of byte streams.
//!
//! One round, per topic:
//!
//! ```text
//! A → B   Heads { topic, heads: A's vector }
//! B → A   Events { topic, events: B's gap for A }
//! A       inserts them, so A's vector now includes B's devices
//! B → A   Heads { topic, heads: B's vector }   (B asks in the other direction)
//! A → B   Events { topic, events: A's gap for B }
//! B       inserts them
//! ```
//!
//! Two request/reply pairs per topic, and both logs hold the same set. Nothing
//! depends on the order within a message: [`little_log_core::EventLog::insert`]
//! is set union, and the projection folds an entry's whole event set regardless
//! of arrival order.
//!
//! This is deliberately *not* iroh-shaped: it takes a reader and a writer, which
//! iroh's `RecvStream`/`SendStream` are, so the exchange is testable over an
//! in-memory duplex and a different transport could carry it unchanged.

use little_log_core::crypto::to_hex;
use little_log_core::invite::Invite;
use little_log_core::model::{JoinState, Role};
use little_log_core::{CoreError, Event, Hlc, Repo, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// The messages peers exchange.
///
/// JSON, newline-delimited: a debug-friendly protocol whose payloads are already
/// sealed ciphertext, so the encoding leaks nothing the events do not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncMessage {
    /// "This is everything I have heard from, for this topic."
    Heads { topic: String, heads: Vec<Hlc> },
    /// "This is what you were missing" — plus the sender's own vector, so the
    /// other side can compute the gap to send back without a second request.
    Events {
        topic: String,
        events: Vec<Event>,
        heads: Vec<Hlc>,
    },
    /// "I would like to join this child's log."
    ///
    /// The invite link carries no key, so this is how a joiner gets in: it can
    /// reach us and ask, and nothing more until a person here says yes.
    JoinRequest {
        topic: String,
        device: String,
        role: Role,
        /// Where to reach the asking device, so the host can dial it back.
        #[serde(default)]
        addr: String,
    },
    /// "Here is where that stands."
    ///
    /// `key` is set only when `state` is `accepted`, and only ever travels back
    /// over the connection the asker opened — which the transport has already
    /// authenticated as that device.
    JoinReply {
        topic: String,
        state: JoinState,
        key: Option<String>,
    },
    /// "I have stored what you sent."
    ///
    /// Without this the dialer cannot know its pushed events landed: closing the
    /// connection the moment it has written them can cut the peer off mid-read,
    /// which silently loses the *other* direction of a round.
    Done,
}

/// What the host said about a join request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinReply {
    pub state: JoinState,
    /// The child's group key, hex, once accepted.
    pub key: Option<String>,
}

/// What one round moved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Events accepted from the peer.
    pub received: usize,
    /// Events sent to the peer (the size of their gap).
    pub sent: usize,
}

/// One direction-safe view of a stream, with the read buffer and line scratch
/// kept together so neither function has to own them separately.
struct Wire<R, W> {
    reader: BufReader<R>,
    writer: W,
    line: String,
}

impl<R, W> Wire<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            line: String::new(),
        }
    }

    async fn send(&mut self, message: &SyncMessage) -> Result<()> {
        let mut text = serde_json::to_string(message)
            .map_err(|error| CoreError::data(format!("could not encode: {error}")))?;
        text.push('\n');
        self.writer
            .write_all(text.as_bytes())
            .await
            .map_err(io_error)?;
        self.writer.flush().await.map_err(io_error)
    }

    /// The next message, or `None` at the end-of-round marker / clean EOF.
    async fn next(&mut self) -> Result<Option<SyncMessage>> {
        self.line.clear();
        if self
            .reader
            .read_line(&mut self.line)
            .await
            .map_err(io_error)?
            == 0
        {
            return Ok(None);
        }
        let trimmed = self.line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        serde_json::from_str(trimmed)
            .map(Some)
            .map_err(|error| CoreError::invalid(format!("bad sync message: {error}")))
    }

    /// Signal that this side has finished.
    async fn finish(&mut self) -> Result<()> {
        self.writer.write_all(b"\n").await.map_err(io_error)?;
        self.writer.flush().await.map_err(io_error)
    }
}

/// Serve one round to a peer that is driving the exchange.
///
/// The listening side: answers `Heads` with the gap, and stores `Events`.
pub async fn serve_round<R, W>(reader: R, writer: W, repo: &mut Repo) -> Result<SyncReport>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut wire = Wire::new(reader, writer);
    let mut report = SyncReport::default();

    while let Some(message) = wire.next().await? {
        match message {
            SyncMessage::Done => break,
            SyncMessage::Heads { topic, heads } => {
                let gap = repo.log().missing_for_topic(&topic, &heads)?;
                let mine = repo.log().heads_for_topic(&topic)?;
                report.sent += gap.len();
                wire.send(&SyncMessage::Events {
                    topic,
                    events: gap,
                    heads: mine,
                })
                .await?;
            }
            SyncMessage::Events { topic, events, .. } => {
                report.received += accept(repo, &topic, events)?;
            }
            SyncMessage::JoinRequest {
                topic,
                device,
                role,
                addr,
            } => {
                // Record it and answer with where it stands. Whether to let this
                // device in is a person's call, so the answer is usually "still
                // waiting" and the joiner asks again.
                repo.save_join_request(&topic, &device, role)?;
                if !addr.is_empty() {
                    // The invite only taught the joiner where we are. Saving the
                    // asker's own ticket makes the address book symmetric, so
                    // either phone can start the next round.
                    repo.save_peer(&topic, &addr)?;
                }
                let state = repo
                    .join_state(&topic, &device)?
                    .unwrap_or(JoinState::Pending);
                let key = match state {
                    JoinState::Accepted => repo
                        .group_key(&topic)?
                        .map(|group| to_hex(&group.to_bytes())),
                    _ => None,
                };
                wire.send(&SyncMessage::JoinReply {
                    topic: topic.clone(),
                    state,
                    key,
                })
                .await?;
                // A decline answers *this* attempt, not every future one. The
                // joiner stops polling when it hears it, so forgetting it here
                // lets a reconnect start from pending instead of replaying the
                // rejection forever.
                if state == JoinState::Declined {
                    repo.clear_join_request(&topic, &device)?;
                }
            }
            SyncMessage::JoinReply { .. } => {
                return Err(CoreError::invalid(
                    "a peer answered with a join reply nobody asked for",
                ));
            }
        }
    }

    // Acknowledge, so the dialer knows its last batch was stored rather than
    // merely written to a socket.
    wire.send(&SyncMessage::Done).await?;
    wire.finish().await?;
    Ok(report)
}

/// Drive one round against a peer, for every topic this device takes part in.
///
/// Asks in both directions, so neither device has to be "the server": whoever
/// calls this pushes its own gap as well as pulling the peer's.
pub async fn sync_round<R, W>(
    reader: R,
    writer: W,
    repo: &mut Repo,
    topics: &[String],
) -> Result<SyncReport>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut wire = Wire::new(reader, writer);
    let mut report = SyncReport::default();

    for topic in topics {
        // Pull: our vector out, theirs back, along with whatever we were missing.
        let mine = repo.log().heads_for_topic(topic)?;
        wire.send(&SyncMessage::Heads {
            topic: topic.clone(),
            heads: mine,
        })
        .await?;

        let Some(SyncMessage::Events {
            events,
            heads: theirs,
            ..
        }) = wire.next().await?
        else {
            return Err(CoreError::invalid(
                "a peer answered a heads request with something else",
            ));
        };
        report.received += accept(repo, topic, events)?;

        // Push: what they were missing, computed from the vector they just sent.
        let gap = repo.log().missing_for_topic(topic, &theirs)?;
        let mine = repo.log().heads_for_topic(topic)?;
        report.sent += gap.len();
        wire.send(&SyncMessage::Events {
            topic: topic.clone(),
            events: gap,
            heads: mine,
        })
        .await?;
    }

    wire.finish().await?;
    // Wait for the peer to confirm it stored everything: the round is not over
    // until both sides agree, and a connection closed early loses the push.
    let _ = wire.next().await?;
    Ok(report)
}

/// Ask to join a child's log, once.
///
/// One round of the ask: someone on the other side has to say yes, so the answer
/// is usually "still waiting" and the caller asks again. The invite is what we
/// ask *about* — it carries no key, so an `accepted` reply is the only way in.
pub async fn ask_to_join<R, W>(
    reader: R,
    writer: W,
    invite: &Invite,
    device: &str,
    addr: &str,
) -> Result<JoinReply>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut wire = Wire::new(reader, writer);
    wire.send(&SyncMessage::JoinRequest {
        topic: invite.topic.clone(),
        device: device.to_string(),
        role: invite.role,
        addr: addr.to_string(),
    })
    .await?;

    let Some(SyncMessage::JoinReply { state, key, .. }) = wire.next().await? else {
        return Err(CoreError::invalid("a peer did not answer a join request"));
    };
    wire.finish().await?;
    Ok(JoinReply { state, key })
}

/// Store events from a peer, and fold each one into the row it describes.
///
/// Three things matter here, all load-bearing:
///
/// - Events for a topic this device holds no key for are ignored. Storing them
///   would grow the log with ciphertext nobody here can ever read, and would let
///   a stranger push unbounded data at us.
/// - [`little_log_core::EventLog::insert`] verifies the signature, so a forged
///   event never lands.
/// - Inserting is not enough on its own: until the event is *projected*, the log
///   holds it and no screen shows it.
fn accept(repo: &Repo, topic: &str, events: Vec<Event>) -> Result<usize> {
    let Some(group) = repo.group_key(topic)? else {
        return Ok(0);
    };

    let mut accepted = 0;
    for event in events {
        if event.topic != topic {
            return Err(CoreError::invalid(
                "a peer sent an event for the wrong topic",
            ));
        }
        // `insert` verifies the signature, so a forged event never lands.
        if repo.log().insert(&event)? {
            accepted += 1;
            // An event in the log is not visible until it is folded: this is what
            // turns a peer's history into rows the screens can read.
            repo.project(&event, &group)?;
        }
    }
    Ok(accepted)
}

fn io_error(error: std::io::Error) -> CoreError {
    CoreError::io("sync stream", error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use little_log_core::clock::Clock;
    use little_log_core::crypto::{DeviceKey, GroupKey};
    use little_log_core::model::{Entry, EntryKind, EventKind};
    use little_log_core::payload::EntryPayload;

    const TOPIC: &str = "baby-1";

    struct Device {
        key: DeviceKey,
        clock: Clock,
        repo: Repo,
    }

    impl Device {
        /// A device that already holds `TOPIC`'s key, as a joiner does after
        /// redeeming an invite.
        fn joined(group: &GroupKey) -> Self {
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

        fn ids(&self) -> Vec<String> {
            let mut ids: Vec<String> = self
                .repo
                .log()
                .all()
                .unwrap()
                .into_iter()
                .map(|event| event.id)
                .collect();
            ids.sort();
            ids
        }
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    /// One round between two devices over an in-memory duplex, in the shape
    /// iroh gives us: a separate reader and writer per side.
    fn round(a: &mut Device, b: &mut Device) -> SyncReport {
        runtime().block_on(async {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let (client_read, client_write) = tokio::io::split(client);
            let (server_read, server_write) = tokio::io::split(server);
            let topics = vec![TOPIC.to_string()];

            let (report, served) = tokio::join!(
                sync_round(client_read, client_write, &mut a.repo, &topics),
                serve_round(server_read, server_write, &mut b.repo),
            );
            served.unwrap();
            report.unwrap()
        })
    }

    /// One ask over an in-memory duplex, the way `join_once` drives it.
    fn ask(repo: &mut Repo, invite: &Invite, device: &str) -> JoinReply {
        runtime().block_on(async {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let (client_read, client_write) = tokio::io::split(client);
            let (server_read, server_write) = tokio::io::split(server);

            let (reply, served) = tokio::join!(
                ask_to_join(client_read, client_write, invite, device, "ticket"),
                serve_round(server_read, server_write, repo),
            );
            // The real accept loop ignores a failed final `Done` when the asker
            // has already closed; the reply is what this test is about.
            let _ = served;
            reply.unwrap()
        })
    }

    /// A decline ends one attempt. The next ask is a new attempt, so it must be
    /// pending again — otherwise reconnecting replays the rejection forever.
    #[test]
    fn a_decline_answers_one_attempt_and_a_retry_starts_pending() {
        let mut host = Device::joined(&GroupKey::generate().unwrap());
        let invite = Invite {
            topic: TOPIC.to_string(),
            role: Role::CoParent,
            by: "inviter".to_string(),
            addr: "ticket".to_string(),
        };

        host.repo
            .save_join_request(TOPIC, "device-1", Role::CoParent)
            .unwrap();
        host.repo
            .resolve_join_request(TOPIC, "device-1", JoinState::Declined)
            .unwrap();

        let first = ask(&mut host.repo, &invite, "device-1");
        assert_eq!(first.state, JoinState::Declined);
        assert!(
            host.repo.join_state(TOPIC, "device-1").unwrap().is_none(),
            "a delivered decline must not be stored as a permanent ban"
        );

        let second = ask(&mut host.repo, &invite, "device-1");
        assert_eq!(
            second.state,
            JoinState::Pending,
            "reconnecting should ask again, not replay the decline"
        );
    }

    #[test]
    fn a_device_that_holds_nothing_pulls_the_whole_history() {
        // This is what joining is: an empty vector, and the gap is everything.
        let group = GroupKey::generate().unwrap();
        let mut parent = Device::joined(&group);
        parent.feed(1_000);
        parent.feed(2_000);

        let mut joiner = Device::joined(&group);
        assert_eq!(joiner.ids().len(), 0);

        let report = round(&mut joiner, &mut parent);

        assert_eq!(report.received, 2);
        assert_eq!(joiner.ids(), parent.ids());
    }

    #[test]
    fn a_round_moves_both_gaps_so_either_side_can_initiate() {
        let group = GroupKey::generate().unwrap();
        let mut phone = Device::joined(&group);
        let mut tablet = Device::joined(&group);
        phone.feed(1_000);
        tablet.feed(2_000);

        let report = round(&mut phone, &mut tablet);

        assert_eq!(report.received, 1, "the phone took the tablet's feed");
        assert_eq!(report.sent, 1, "and gave its own");
        assert_eq!(phone.ids(), tablet.ids(), "both logs hold the same set");
        assert_eq!(phone.ids().len(), 2);
    }

    #[test]
    fn a_second_round_moves_nothing() {
        // Convergence is stable, which is what makes syncing on every app open
        // cheap instead of a full transfer.
        let group = GroupKey::generate().unwrap();
        let mut phone = Device::joined(&group);
        let mut tablet = Device::joined(&group);
        phone.feed(1_000);
        round(&mut phone, &mut tablet);

        assert_eq!(round(&mut phone, &mut tablet), SyncReport::default());
    }

    #[test]
    fn a_stranger_cannot_grow_a_log_with_a_topic_we_hold_no_key_for() {
        // The group key is what makes an event readable. Storing ciphertext for
        // a child this device has nothing to do with would be unbounded storage
        // handed to whoever asks.
        let stranger = GroupKey::generate().unwrap();
        let mut stranger = Device::joined(&stranger);
        stranger.feed(1_000);

        // A device that takes part in a *different* child: it holds a key for
        // baby-2 only, so events for baby-1 are ciphertext it could never open
        // and must not keep.
        let listener = Repo::open_in_memory().unwrap();
        listener
            .save_group_key("baby-2", &GroupKey::generate().unwrap())
            .unwrap();
        let events = stranger.repo.log().all().unwrap();

        let accepted = accept(&listener, "baby-1", events).unwrap();

        assert_eq!(accepted, 0);
        assert_eq!(listener.log().count().unwrap(), 0);
    }

    #[test]
    fn a_forged_event_is_refused() {
        let group = GroupKey::generate().unwrap();
        let mut parent = Device::joined(&group);
        let mut forged = {
            let entry = Entry::new(EntryKind::Feed, 1_000);
            let g = parent.repo.group_key(TOPIC).unwrap().unwrap();
            Event::seal(
                &parent.key,
                &g,
                TOPIC,
                &entry.id,
                EventKind::Create,
                parent.clock.tick(1_000),
                &EntryPayload::of(&entry).to_bytes().unwrap(),
            )
            .unwrap()
        };
        // Flip a bit of the sealed payload: the signature no longer covers it.
        forged.payload[0] ^= 0x01;

        let listener = Device::joined(&group);
        assert!(accept(&listener.repo, TOPIC, vec![forged]).is_err());
        assert_eq!(listener.ids().len(), 0);
    }
}
