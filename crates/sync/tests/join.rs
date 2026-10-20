//! The join, end to end: one device invites, another redeems and catches up.
//!
//! This is the flow the app's `create_invite` and `join_invite` commands drive,
//! minus the IPC layer between them. It is deliberately an integration test over
//! a real QUIC connection rather than a unit test over the pieces, because the
//! pieces passing individually is exactly what would hide a wiring mistake.

use little_log_core::clock::Clock;
use little_log_core::crypto::{DeviceKey, GroupKey};
use little_log_core::invite::Invite;
use little_log_core::model::{Entry, EntryKind, JoinState, Role};
use little_log_core::Repo;
use little_log_sync::{decode_addr, encode_addr, join_once, serve, sync_once, Endpoint, Relays};

/// A device with its own endpoint, as the app has: bound once, kept alive.
struct Device {
    key: DeviceKey,
    clock: Clock,
    repo: Repo,
    endpoint: Endpoint,
    /// Keeps the accept loop alive for the duration of the test.
    _router: Option<little_log_sync::Router>,
}

impl Device {
    async fn with_database(database: Option<std::path::PathBuf>) -> Self {
        let repo = match &database {
            Some(path) => {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                Repo::open(path).unwrap()
            }
            None => Repo::open_in_memory().unwrap(),
        };
        let key = repo.device_key().unwrap();
        let clock = Clock::new(key.id());
        let endpoint = little_log_sync::bind(key.secret_bytes(), Relays::Disabled)
            .await
            .unwrap();
        let router = database.map(|path| serve(endpoint.clone(), path));
        Self {
            key,
            clock,
            repo,
            endpoint,
            _router: router,
        }
    }

    fn feed(&mut self, at: i64, note: &str) -> Entry {
        let baby = self.repo.babies().unwrap().first().cloned().unwrap();
        let mut entry = Entry::new(EntryKind::Feed, at);
        entry.baby_id = baby.id;
        entry.note = Some(note.to_string());
        self.repo
            .record_new_entry(&self.key, &mut self.clock, &entry)
            .unwrap()
    }
}

/// Two phones, one household: the second one joins from a link and ends up with
/// the same child and the same history.
#[tokio::test]
async fn a_device_that_redeems_an_invite_ends_up_with_the_same_log() {
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let directory =
            std::env::temp_dir().join(format!("little-log-join-{}", std::process::id()));
        let database = directory.join("parent.sqlite3");

        // The parent: a child, a group key, and two feeds already logged.
        let mut parent = Device::with_database(Some(database.clone())).await;
        let baby = parent
            .repo
            .create_baby(&parent.key, &mut parent.clock, "Ada", "2026-02-14")
            .unwrap();
        parent.feed(1_000, "first");
        parent.feed(2_000, "second");

        // The invite: the child, the key, the role, and where to find us.
        let ticket = encode_addr(&parent.endpoint.addr());
        let link = Invite::mint(&parent.repo, Role::CoParent, ticket)
            .unwrap()
            .encode();

        // The other phone: nothing but the link.
        let mut joiner = Device::with_database(None).await;
        assert!(joiner.repo.babies().unwrap().is_empty());
        let joiner_ticket = encode_addr(&joiner.endpoint.addr());

        let invite = Invite::decode(&link).unwrap();
        invite.remember(&joiner.repo).unwrap();

        // Ask. Nobody has looked at the parent yet, so the answer is "waiting" —
        // and the link on its own has got the joiner nowhere.
        let device = joiner.repo.device_key().unwrap().id();
        let addr = decode_addr(&invite.addr).unwrap();
        let asked = join_once(
            &joiner.endpoint,
            addr.clone(),
            &invite,
            &device,
            &joiner_ticket,
        )
        .await
        .unwrap();
        assert_eq!(asked.state, JoinState::Pending);
        assert!(asked.key.is_none(), "a pending reply carries no key");
        assert!(
            joiner.repo.group_key(&invite.topic).unwrap().is_none(),
            "and the joiner still cannot read a thing"
        );
        assert!(
            parent
                .repo
                .peers_for(&invite.topic)
                .unwrap()
                .contains(&joiner_ticket),
            "the host learned where to reach the joiner from the ask"
        );

        // The parent's owner taps Accept.
        parent
            .repo
            .resolve_join_request(&invite.topic, &device, JoinState::Accepted)
            .unwrap();

        // The joiner asks again, and this time is let in.
        let granted = join_once(
            &joiner.endpoint,
            addr.clone(),
            &invite,
            &device,
            &joiner_ticket,
        )
        .await
        .unwrap();
        assert_eq!(granted.state, JoinState::Accepted);
        invite
            .accept_key(&joiner.repo, granted.key.as_deref().expect("the key"))
            .unwrap();

        let report = sync_once(
            &joiner.endpoint,
            addr,
            &mut joiner.repo,
            std::slice::from_ref(&invite.topic),
        )
        .await
        .unwrap();

        assert_eq!(
            report.received,
            3,
            "the round carried {report:?}; the joiner holds {} events",
            joiner.repo.log().count().unwrap()
        );
        let joined = joiner.repo.baby().unwrap().expect("the child arrived");
        let entries = joiner.repo.entries().unwrap();
        let notes: Vec<String> = entries.iter().filter_map(|e| e.note.clone()).collect();
        let _ = std::fs::remove_dir_all(&directory);

        (
            report.received,
            joined.name,
            joined.id == baby.id,
            entries.len(),
            notes,
        )
    })
    .await
    .expect("a direct connection must not need the network");

    let (received, name, same_child, entry_count, notes) = outcome;
    assert_eq!(name, "Ada", "the child arrived as profile events");
    assert!(same_child, "and it is the same child, not a lookalike");
    assert_eq!(received, 3, "two feeds and the profile");
    assert_eq!(entry_count, 2);
    assert_eq!(notes, vec!["second", "first"], "newest first");
}

/// A device that redeems an invite is a *peer*, not a copy: it keeps its own log
/// and the parent keeps its own, and both keep working alone.
#[tokio::test]
async fn redeeming_does_not_take_the_log_over() {
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let directory =
            std::env::temp_dir().join(format!("little-log-join2-{}", std::process::id()));
        let database = directory.join("parent.sqlite3");
        let mut parent = Device::with_database(Some(database.clone())).await;
        parent
            .repo
            .create_baby(&parent.key, &mut parent.clock, "Ada", "")
            .unwrap();
        parent.feed(1_000, "from the parent");

        let link = Invite::mint(
            &parent.repo,
            Role::Caregiver,
            encode_addr(&parent.endpoint.addr()),
        )
        .unwrap()
        .encode();

        let mut joiner = Device::with_database(None).await;
        let joiner_ticket = encode_addr(&joiner.endpoint.addr());
        let invite = Invite::decode(&link).unwrap();
        invite.remember(&joiner.repo).unwrap();

        // Accept it, then ask and take the key, as the join flow does.
        let device = joiner.repo.device_key().unwrap().id();
        let addr = decode_addr(&invite.addr).unwrap();

        // It has to ask first: accepting a device that never asked would be a way
        // to hand the key to an id somebody guessed.
        assert!(
            parent
                .repo
                .resolve_join_request(&invite.topic, &device, JoinState::Accepted)
                .is_err(),
            "there is no request to accept yet"
        );
        let asked = join_once(
            &joiner.endpoint,
            addr.clone(),
            &invite,
            &device,
            &joiner_ticket,
        )
        .await
        .unwrap();
        assert_eq!(asked.state, JoinState::Pending);

        parent
            .repo
            .resolve_join_request(&invite.topic, &device, JoinState::Accepted)
            .unwrap();
        let granted = join_once(
            &joiner.endpoint,
            addr.clone(),
            &invite,
            &device,
            &joiner_ticket,
        )
        .await
        .unwrap();
        invite
            .accept_key(&joiner.repo, granted.key.as_deref().expect("the key"))
            .unwrap();

        sync_once(
            &joiner.endpoint,
            addr.clone(),
            &mut joiner.repo,
            std::slice::from_ref(&invite.topic),
        )
        .await
        .unwrap();

        // The joiner logs something of its own, then syncs again.
        joiner.feed(3_000, "from the joiner");
        let second = sync_once(
            &joiner.endpoint,
            addr,
            &mut joiner.repo,
            std::slice::from_ref(&invite.topic),
        )
        .await
        .unwrap();
        assert_eq!(second.sent, 1, "the joiner should push its own feed");

        let parent_after = Repo::open(&database).unwrap();
        let notes: Vec<String> = parent_after
            .entries()
            .unwrap()
            .iter()
            .filter_map(|e| e.note.clone())
            .collect();
        let _ = std::fs::remove_dir_all(&directory);
        (notes, joiner.repo.entries().unwrap().len())
    })
    .await
    .expect("a direct connection must not need the network");

    let (parent_notes, joiner_entries) = outcome;
    assert_eq!(
        parent_notes,
        vec!["from the joiner", "from the parent"],
        "the parent got it"
    );
    assert_eq!(joiner_entries, 2);
}

/// The key an invite carries is what makes the child readable; without it the
/// events are stored ciphertext. This is the property the whole design rests on.
#[test]
fn without_the_key_the_same_events_are_unreadable() {
    let repo = Repo::open_in_memory().unwrap();
    let key = repo.device_key().unwrap();
    let mut clock = Clock::new(key.id());
    let baby = repo.create_baby(&key, &mut clock, "Ada", "").unwrap();
    let topic = baby.id.clone().unwrap();
    let ours = repo.group_key(&topic).unwrap().unwrap();
    let theirs = GroupKey::generate().unwrap();

    let events = repo.log().for_entry(&topic).unwrap();
    assert_eq!(events.len(), 1, "a profile event");
    assert!(
        events[0].open(&ours).is_ok(),
        "whoever holds the key can read it"
    );
    assert!(events[0].open(&theirs).is_err(), "and nobody else can");
}
