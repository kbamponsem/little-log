//! Signed, encrypted events — the only thing the append-only log stores.
//!
//! An event is never mutated after it is written. The readable headers (`id`,
//! `topic`, `author`, `hlc`, `kind`) let a peer sync without the group key; the
//! payload is sealed with that key; and the signature covers **both**, so a peer
//! cannot re-attribute, re-time or edit an event it received.

use serde::{Deserialize, Serialize};

use crate::clock::Hlc;
use crate::crypto::{self, DeviceKey, GroupKey, NONCE_LEN, SIGNATURE_LEN};
use crate::error::Result;
use crate::model::{new_id, EventKind};

/// Marks the signing layout below, so a future format change cannot be mistaken
/// for a valid signature from this one.
const MAGIC: &[u8; 4] = b"LLEV";
/// 1 → 2 added `entry_id` to the signed headers.
const FORMAT_VERSION: u8 = 2;

/// Transport encoding for the fixed-size byte arrays, which are longer than
/// serde's built-in array support.
///
/// Length-checked on the way in: a truncated signature has to be an error, not
/// something quietly padded into a valid-looking one.
mod bytes_array {
    use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S, const N: usize>(value: &[u8; N], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.as_slice().serialize(serializer)
    }

    pub fn deserialize<'de, D, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        bytes
            .try_into()
            .map_err(|_| D::Error::custom("a fixed-size field arrived with the wrong length"))
    }
}

/// One immutable event in the log.
///
/// `Serialize`/`Deserialize` here are for **transport only** — the bytes a peer
/// sends and receives. They are not the signed bytes: [`Event::signing_bytes`]
/// is laid out by hand and pinned by a golden test, so changing this encoding
/// cannot change a signature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// UUIDv7, generated on the authoring device. Identifies *this* event.
    pub id: String,
    /// The entry this event is about — `create`, `edit` and `delete` for one
    /// logical entry all carry the same one, which is how the projection groups
    /// them. A readable header, so a peer can filter without the group key.
    pub entry_id: String,
    /// The baby this event belongs to.
    pub topic: String,
    /// The authoring device's id — the hex of its public key.
    pub author: String,
    /// The author's hybrid logical clock reading.
    pub hlc: Hlc,
    pub kind: EventKind,
    /// Nonce for `payload`; unique per event.
    #[serde(with = "bytes_array")]
    pub nonce: [u8; NONCE_LEN],
    /// The sealed payload.
    pub payload: Vec<u8>,
    /// The author's signature over [`Event::signing_bytes`].
    #[serde(with = "bytes_array")]
    pub signature: [u8; SIGNATURE_LEN],
}

impl Event {
    /// Build, seal and sign an event in one step.
    ///
    /// The readable headers are used as the AEAD associated data, so the sealed
    /// payload is bound to *this* event and cannot be replayed under another.
    pub fn seal(
        key: &DeviceKey,
        group: &GroupKey,
        topic: impl Into<String>,
        entry_id: impl Into<String>,
        kind: EventKind,
        hlc: Hlc,
        plaintext: &[u8],
    ) -> Result<Self> {
        let mut event = Self {
            id: new_id(),
            entry_id: entry_id.into(),
            topic: topic.into(),
            author: key.id(),
            hlc,
            kind,
            nonce: [0; NONCE_LEN],
            payload: Vec::new(),
            signature: [0; SIGNATURE_LEN],
        };

        let (nonce, payload) = group.seal(&event.headers_bytes(), plaintext)?;
        event.nonce = nonce;
        event.payload = payload;
        event.signature = key.sign(&event.signing_bytes());
        Ok(event)
    }

    /// Open the payload with the baby's group key.
    pub fn open(&self, group: &GroupKey) -> Result<Vec<u8>> {
        group.open(&self.headers_bytes(), &self.nonce, &self.payload)
    }

    /// Check the signature against the key this event names as its author.
    pub fn verify(&self) -> Result<()> {
        crypto::verify_device(&self.author, &self.signing_bytes(), &self.signature)
    }

    /// The exact bytes the signature covers: the readable headers, then the
    /// nonce and the sealed payload.
    ///
    /// Written out by hand rather than derived from a serialiser. A signature
    /// has to be byte-identical on every device and every future release, and
    /// a serialiser's output is a dependency's decision — this layout is ours,
    /// and a test locks it.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = self.headers_bytes();
        bytes.extend_from_slice(&self.nonce);
        push_bytes(&mut bytes, &self.payload);
        bytes
    }

    /// The readable headers. Signed, and used as the AEAD associated data.
    fn headers_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(FORMAT_VERSION);
        push_bytes(&mut bytes, self.id.as_bytes());
        push_bytes(&mut bytes, self.entry_id.as_bytes());
        push_bytes(&mut bytes, self.topic.as_bytes());
        push_bytes(&mut bytes, self.author.as_bytes());
        bytes.extend_from_slice(&self.hlc.wall_ms.to_be_bytes());
        bytes.extend_from_slice(&self.hlc.counter.to_be_bytes());
        push_bytes(&mut bytes, self.hlc.node.as_bytes());
        push_bytes(&mut bytes, self.kind.as_str().as_bytes());
        bytes
    }
}

/// Length-prefixed, big-endian, no padding: the same bytes on every platform.
fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("event field longer than 4 GiB");
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EntryKind;

    fn keys() -> (DeviceKey, GroupKey) {
        (
            DeviceKey::generate().unwrap(),
            GroupKey::generate().unwrap(),
        )
    }

    fn event() -> (DeviceKey, GroupKey, Event) {
        let (key, group) = keys();
        let event = Event::seal(
            &key,
            &group,
            "baby-1",
            "entry-1",
            EventKind::Create,
            Hlc::new(1_000, 0, key.id()),
            br#"{"kind":"feed","amount":120}"#,
        )
        .unwrap();
        (key, group, event)
    }

    #[test]
    fn a_sealed_event_verifies_and_opens() {
        let (_, group, event) = event();

        assert!(event.verify().is_ok());
        assert_eq!(
            event.open(&group).unwrap(),
            br#"{"kind":"feed","amount":120}"#
        );
        assert_ne!(event.payload, br#"{"kind":"feed","amount":120}"#);
    }

    #[test]
    fn the_author_signs_its_own_events() {
        let (key, _, event) = event();

        assert_eq!(event.author, key.id());
    }

    #[test]
    fn a_tampered_payload_fails_verification() {
        let (_, _, mut event) = event();
        event.payload[0] ^= 0x01;

        assert!(event.verify().is_err());
    }

    #[test]
    fn a_tampered_header_fails_verification() {
        // The point of signing the headers: a peer that can see them cannot
        // quietly retime or reclassify an event it is relaying.
        let (_, _, event) = event();

        let mut retimed = event.clone();
        retimed.hlc.counter += 1;
        assert!(retimed.verify().is_err(), "counter is covered");

        let mut rekinded = event.clone();
        rekinded.kind = EventKind::Delete;
        assert!(rekinded.verify().is_err(), "kind is covered");

        let mut re_topic = event.clone();
        re_topic.topic = "baby-2".into();
        assert!(re_topic.verify().is_err(), "topic is covered");

        let mut re_named = event.clone();
        re_named.id = crate::model::new_id();
        assert!(re_named.verify().is_err(), "id is covered");
    }

    #[test]
    fn re_attributing_an_event_to_another_device_fails() {
        let (_, _, event) = event();
        let impostor = DeviceKey::generate().unwrap();

        let mut forged = event;
        forged.author = impostor.id();

        assert!(forged.verify().is_err());
    }

    #[test]
    fn a_payload_cannot_be_replayed_under_another_events_headers() {
        let (key, group, first) = event();
        let second = Event::seal(
            &key,
            &group,
            "baby-1",
            "entry-1",
            EventKind::Create,
            Hlc::new(2_000, 0, key.id()),
            br#"{"kind":"diaper"}"#,
        )
        .unwrap();

        // Splice the first event's sealed payload into the second.
        let mut spliced = second.clone();
        spliced.payload = first.payload.clone();
        spliced.nonce = first.nonce;

        assert!(
            spliced.open(&group).is_err(),
            "the headers are the associated data, so this must not open"
        );
    }

    #[test]
    fn the_signing_layout_is_stable() {
        // A golden vector: if this test breaks, signatures produced by older
        // devices would stop verifying, so the change is deliberate and
        // versioned (FORMAT_VERSION), never accidental.
        let event = Event {
            id: "x".into(),
            entry_id: "e".into(),
            topic: "t".into(),
            author: "a".into(),
            hlc: Hlc::new(1, 2, "n"),
            kind: EventKind::Create,
            nonce: [7; NONCE_LEN],
            payload: vec![9, 9],
            signature: [0; SIGNATURE_LEN],
        };

        let mut expected = Vec::new();
        fn field(out: &mut Vec<u8>, bytes: &[u8]) {
            out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            out.extend_from_slice(bytes);
        }

        expected.extend_from_slice(b"LLEV");
        expected.push(2);
        field(&mut expected, b"x"); // id
        field(&mut expected, b"e"); // entry_id
        field(&mut expected, b"t"); // topic
        field(&mut expected, b"a"); // author
        expected.extend_from_slice(&1i64.to_be_bytes()); // wall_ms
        expected.extend_from_slice(&2u32.to_be_bytes()); // counter
        field(&mut expected, b"n"); // node
        field(&mut expected, b"create"); // kind
        expected.extend_from_slice(&[7; NONCE_LEN]);
        field(&mut expected, &[9, 9]); // payload

        assert_eq!(event.signing_bytes(), expected);
    }

    #[test]
    fn the_entry_kind_enum_is_untouched_by_any_of_this() {
        // Cheap guard that the log's kind ids and the projection's entry kinds
        // stay distinct concepts.
        assert_eq!(EntryKind::Feed.as_str(), "feed");
        assert_eq!(EventKind::Create.as_str(), "create");
    }
}
