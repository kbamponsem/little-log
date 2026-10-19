//! Invite links: asking to join one child's log.
//!
//! An invite carries four things — the child's id (which is their topic), the
//! role being offered, the device doing the inviting, and an address at which
//! that device can be reached. The last one is opaque here on purpose: this crate
//! has no transport, so it carries whatever ticket the sync layer minted.
//!
//! **The link deliberately carries no key.** It used to, which made it a bearer
//! token: whoever held it could read that child's whole log. That left no room
//! for the host to accept anyone, because the joiner already had everything. So
//! the key stays with the device that holds it, the link only gets the joiner to
//! the door, and [`Invite`] is what a joiner asks with — see
//! [`crate::model::JoinState`] and `Repo::save_join_request`. The host answers on
//! a later poll with the key, over the connection it has already authenticated.
//!
//! Nothing here is secret any more, so `Debug` is derived normally.

use crate::crypto::{self, GROUP_KEY_LEN};
use crate::error::{CoreError, Result};
use crate::model::Role;
use crate::repo::Repo;

/// The prefix that makes a link recognisable, and tells us a string is ours.
pub const SCHEME: &str = "little-log://invite";

/// An invite, ready to encode into a link or to ask with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    /// The child's id, which is also the topic their events travel on.
    pub topic: String,
    pub role: Role,
    /// The device making the invitation.
    pub by: String,
    /// Where to reach that device, as the transport spells it.
    pub addr: String,
}

impl Invite {
    /// Mint an invite for the child this device is looking at.
    ///
    /// `addr` is this device's reachable address, spelled by the transport. It is
    /// opaque here — but it must be safe inside a query string, which the field
    /// separators `&` and `=` are not.
    ///
    /// Fails when there is no child, or when this device does not hold their
    /// key: without it there would be nothing to hand over once someone accepts.
    pub fn mint(repo: &Repo, role: Role, addr: impl Into<String>) -> Result<Self> {
        let addr = addr.into();
        if addr.is_empty() || addr.contains(['&', '=', '?', '#']) {
            return Err(CoreError::invalid(
                "an address must be a single url-safe value",
            ));
        }
        let baby = repo
            .current_baby()?
            .ok_or_else(|| CoreError::invalid("there is no child to invite anyone to"))?;
        let topic = baby
            .id
            .ok_or_else(|| CoreError::invalid("that child has no id"))?;
        // Not embedded — but the host has to be able to answer, so refuse to
        // promise access this device could not actually grant.
        repo.group_key(&topic)?
            .ok_or_else(|| CoreError::invalid("this device does not hold that child's key"))?;

        Ok(Self {
            topic,
            role,
            by: repo.device_key()?.id(),
            addr,
        })
    }

    /// The link to copy and send: `little-log://invite?topic=…&role=…&by=…&addr=…`
    pub fn encode(&self) -> String {
        format!(
            "{SCHEME}?topic={}&role={}&by={}&addr={}",
            self.topic,
            self.role.as_str(),
            self.by,
            self.addr,
        )
    }

    /// Read a link back. Strict on purpose: an invite that half-parsed would ask
    /// to join the wrong child, or none.
    pub fn decode(link: &str) -> Result<Self> {
        let query = link
            .trim()
            .strip_prefix(SCHEME)
            .and_then(|rest| rest.strip_prefix('?'))
            .ok_or_else(|| CoreError::invalid("that is not a Little Log invite"))?;

        let mut topic = None;
        let mut role = None;
        let mut by = None;
        let mut addr = None;

        for field in query.split('&') {
            let (name, value) = field
                .split_once('=')
                .ok_or_else(|| CoreError::invalid("an invite field has no value"))?;
            match name {
                "topic" if !value.is_empty() => topic = Some(value.to_string()),
                "role" => role = Some(Role::parse(value)?),
                "by" if !value.is_empty() => by = Some(value.to_string()),
                "addr" if !value.is_empty() => addr = Some(value.to_string()),
                other => {
                    return Err(CoreError::invalid(format!(
                        "unexpected invite field {other}"
                    )));
                }
            }
        }

        Ok(Self {
            topic: topic.ok_or_else(|| CoreError::invalid("the invite has no topic"))?,
            role: role.ok_or_else(|| CoreError::invalid("the invite has no role"))?,
            by: by.ok_or_else(|| CoreError::invalid("the invite has no sender"))?,
            addr: addr.ok_or_else(|| CoreError::invalid("the invite has no address"))?,
        })
    }
}

impl Invite {
    /// Remember where the inviter is, so the request has somewhere to go.
    ///
    /// Deliberately *not* a key: this is what a joiner does before anyone has
    /// accepted, and it grants nothing on its own.
    pub fn remember(&self, repo: &Repo) -> Result<()> {
        repo.save_peer(&self.topic, &self.addr)?;
        Ok(())
    }

    /// Take the child's key, once someone on the other side has accepted.
    ///
    /// Only call this on an accepted request: it is the point at which the
    /// joiner can read that child, and the only thing that grants it.
    pub fn accept_key(&self, repo: &Repo, key: &str) -> Result<()> {
        let bytes = crypto::from_hex(key)
            .and_then(|bytes| <[u8; GROUP_KEY_LEN]>::try_from(bytes).ok())
            .ok_or_else(|| CoreError::invalid("the accepted key is not a 32-byte key"))?;

        repo.save_group_key(&self.topic, &crypto::GroupKey::from_bytes(bytes))?;
        repo.save_peer(&self.topic, &self.addr)?;
        // No child row is written here. The child arrives as *events* when the
        // first sync round pulls the profile, exactly as it does for any peer
        // catching up — inventing a local row would give us a child with a name
        // we made up. Pointing at the topic first is safe: the pointer resolves
        // against whatever children exist, so until the round lands it falls back
        // to the first child, and afterwards it names the right one.
        repo.set_current_baby(&self.topic)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Clock;

    /// A device with a child of its own, which is what an invite needs.
    fn device() -> (Repo, String) {
        let repo = Repo::open_in_memory().unwrap();
        let key = repo.device_key().unwrap();
        let mut clock = Clock::new(key.id());
        let baby = repo
            .create_baby(&key, &mut clock, "Ada", "2026-02-14")
            .unwrap();
        (repo, baby.id.unwrap())
    }

    #[test]
    fn an_invite_round_trips_through_its_link() {
        let (repo, topic) = device();
        let invite = Invite::mint(&repo, Role::CoParent, "ticket").unwrap();

        let decoded = Invite::decode(&invite.encode()).unwrap();

        assert_eq!(decoded, invite);
        assert_eq!(decoded.topic, topic);
        assert_eq!(decoded.role, Role::CoParent);
        assert_eq!(decoded.by, repo.device_key().unwrap().id());
    }

    #[test]
    fn a_link_carries_no_key_at_all() {
        // The accept phase is only real if the link cannot already open the log:
        // anyone who saw it would otherwise have that child's whole history.
        let (repo, topic) = device();
        let link = Invite::mint(&repo, Role::Caregiver, "ticket")
            .unwrap()
            .encode();

        let key = crypto::to_hex(&repo.group_key(&topic).unwrap().unwrap().to_bytes());
        assert!(!link.contains(&key), "the link must not carry the key");
        assert!(!link.contains("key="));
    }

    #[test]
    fn the_link_looks_like_a_link() {
        let (repo, topic) = device();
        let link = Invite::mint(&repo, Role::CoParent, "ticket")
            .unwrap()
            .encode();

        assert!(link.starts_with("little-log://invite?"));
        assert!(link.contains(&format!("topic={topic}")));
        assert!(link.contains("role=co_parent"));
    }

    #[test]
    fn a_foreign_or_truncated_link_is_refused_rather_than_half_read() {
        let (repo, _) = device();
        let good = Invite::mint(&repo, Role::CoParent, "ticket")
            .unwrap()
            .encode();
        let missing_role = good.replace("&role=co_parent", "");
        let missing_addr = good.replace("&addr=ticket", "");

        for bad in [
            "",
            "https://example.com/invite",
            "little-log://invite",
            missing_role.as_str(),
            missing_addr.as_str(),
            // The old format, which carried the key. A link may not grant access
            // any more, so one that tries to is refused rather than ignored.
            "little-log://invite?topic=t&key=0000000000000000000000000000000000000000000000000000000000000000&role=co_parent&by=w&addr=ticket",
            // Unknown field, missing topic, unknown role.
            "little-log://invite?topic=t&role=co_parent&by=w&addr=x&extra=1",
            "little-log://invite?role=co_parent&by=w&addr=x",
            "little-log://invite?topic=t&role=not_a_role&by=w&addr=x",
        ] {
            assert!(Invite::decode(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn remembering_an_invite_grants_nothing() {
        // Before anyone accepts, all the joiner has is somewhere to ask.
        let (parent, topic) = device();
        let link = Invite::mint(&parent, Role::CoParent, "ticket")
            .unwrap()
            .encode();

        let joiner = Repo::open_in_memory().unwrap();
        Invite::decode(&link).unwrap().remember(&joiner).unwrap();

        assert_eq!(joiner.peers_for(&topic).unwrap(), vec!["ticket"]);
        assert!(
            joiner.group_key(&topic).unwrap().is_none(),
            "no key until the host accepts"
        );
        assert!(joiner.babies().unwrap().is_empty());
    }

    #[test]
    fn accepting_takes_the_key_and_the_address_but_invents_no_child() {
        // The joiner has never seen this child: nothing about them may be
        // guessed locally, or the log would gain a second, made-up child.
        let (parent, topic) = device();
        let invite = Invite::mint(&parent, Role::CoParent, "ticket").unwrap();
        let key = crypto::to_hex(&parent.group_key(&topic).unwrap().unwrap().to_bytes());

        let joiner = Repo::open_in_memory().unwrap();
        invite.accept_key(&joiner, &key).unwrap();

        assert_eq!(
            joiner.group_key(&topic).unwrap().unwrap().to_bytes(),
            parent.group_key(&topic).unwrap().unwrap().to_bytes(),
            "the joiner can now open that child's events"
        );
        assert_eq!(joiner.peers_for(&topic).unwrap(), vec!["ticket"]);
        assert!(
            joiner.babies().unwrap().is_empty(),
            "the child arrives by sync, not by guesswork"
        );
    }

    #[test]
    fn a_key_that_is_not_a_key_is_refused() {
        let (parent, _) = device();
        let invite = Invite::mint(&parent, Role::CoParent, "ticket").unwrap();
        let joiner = Repo::open_in_memory().unwrap();

        assert!(invite.accept_key(&joiner, "not-hex").is_err());
        assert!(invite.accept_key(&joiner, "abcd").is_err());
        assert!(joiner.group_key(&invite.topic).unwrap().is_none());
    }

    #[test]
    fn inviting_needs_a_child_whose_key_we_hold() {
        let repo = Repo::open_in_memory().unwrap();

        assert!(
            Invite::mint(&repo, Role::CoParent, "ticket").is_err(),
            "no child, nothing to invite anyone to"
        );
    }
}
