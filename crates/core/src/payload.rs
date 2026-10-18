//! What an event carries once it is decrypted: the authored state of one entry.
//!
//! Deliberately narrower than [`Entry`]. An entry's `id`, `baby_id`,
//! `caregiver_id`, `created_at`, `updated_at` and `deleted` are all decided by
//! the projection from the events that win, so allowing them into a payload
//! would let a device claim an author or a timestamp that is not its own.
//!
//! Encoded as JSON, and it is worth being explicit about why canonical form does
//! not matter here: the signature covers the *sealed ciphertext*, which is fixed
//! the moment the event is signed. Nothing can re-encode these bytes behind the
//! signature's back, so the payload needs no canonical encoding of its own.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::model::{
    Amount, AmountUnit, Consistency, DiaperType, Entry, EntryKind, FeedKind, Millis, PeeShade,
    PoopColor, SleepLocation, WakeMood,
};

/// Everything a caregiver can set on an entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntryPayload {
    pub kind: EntryKind,
    pub occurred_at: Millis,
    #[serde(default)]
    pub ended_at: Option<Millis>,
    #[serde(default)]
    pub feed_kind: Option<FeedKind>,
    #[serde(default)]
    pub amount: Option<i64>,
    #[serde(default)]
    pub amount_unit: Option<AmountUnit>,
    #[serde(default)]
    pub diaper_type: Option<DiaperType>,
    #[serde(default)]
    pub pee_shade: Option<PeeShade>,
    #[serde(default)]
    pub pee_amount: Option<Amount>,
    #[serde(default)]
    pub poop_color: Option<PoopColor>,
    #[serde(default)]
    pub consistency: Option<Consistency>,
    #[serde(default)]
    pub poop_amount: Option<Amount>,
    #[serde(default)]
    pub sleep_location: Option<SleepLocation>,
    #[serde(default)]
    pub wake_mood: Option<WakeMood>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub photo: Option<String>,
}

impl EntryPayload {
    /// The authored half of an entry that already exists — what an edit starts
    /// from, so an edit event carries the whole state rather than a diff.
    pub fn of(entry: &Entry) -> Self {
        Self {
            kind: entry.kind,
            occurred_at: entry.occurred_at,
            ended_at: entry.ended_at,
            feed_kind: entry.feed_kind,
            amount: entry.amount,
            amount_unit: entry.amount_unit,
            diaper_type: entry.diaper_type,
            pee_shade: entry.pee_shade,
            pee_amount: entry.pee_amount,
            poop_color: entry.poop_color,
            consistency: entry.consistency,
            poop_amount: entry.poop_amount,
            sleep_location: entry.sleep_location,
            wake_mood: entry.wake_mood,
            note: entry.note.clone(),
            photo: entry.photo.clone(),
        }
    }

    /// Encode for sealing into an event.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|error| CoreError::data(format!("could not encode the entry: {error}")))
    }

    /// Decode a payload that arrived in an event.
    ///
    /// Unknown fields are ignored and missing ones default, so a device running
    /// a newer build can add a field without an older one choking on it.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes)
            .map_err(|error| CoreError::invalid(format!("entry payload is not valid: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        let mut entry = Entry::new(EntryKind::Feed, 1_700_000_000_000);
        entry.feed_kind = Some(FeedKind::Bottle);
        entry.amount = Some(120);
        entry.amount_unit = Some(AmountUnit::Milliliters);
        entry.note = Some("took it all".into());
        entry
    }

    #[test]
    fn a_payload_round_trips_through_its_bytes() {
        let payload = EntryPayload::of(&entry());

        let decoded = EntryPayload::from_bytes(&payload.to_bytes().unwrap()).unwrap();

        assert_eq!(decoded, payload);
        assert_eq!(decoded.amount, Some(120));
        assert_eq!(decoded.note.as_deref(), Some("took it all"));
    }

    #[test]
    fn a_payload_carries_no_identity_or_bookkeeping() {
        // The point of the narrower type: nothing here can claim an author, an
        // id or a timestamp that the event's headers do not already say.
        let payload = EntryPayload::of(&entry());
        let json = String::from_utf8(payload.to_bytes().unwrap()).unwrap();

        for forbidden in [
            "caregiver_id",
            "created_at",
            "updated_at",
            "deleted",
            "baby_id",
        ] {
            assert!(
                !json.contains(forbidden),
                "{forbidden} must not be authored"
            );
        }
    }

    #[test]
    fn only_the_kind_and_time_are_required() {
        // Forward and backward compatibility: every other field may be absent.
        let decoded = EntryPayload::from_bytes(br#"{"kind":"sleep","occurred_at":42}"#).unwrap();

        assert_eq!(decoded.kind, EntryKind::Sleep);
        assert_eq!(decoded.occurred_at, 42);
        assert_eq!(decoded.ended_at, None);
        assert_eq!(decoded.note, None);
    }

    #[test]
    fn an_unknown_field_is_ignored_rather_than_fatal() {
        // A newer device may set a field this build has never heard of.
        let decoded =
            EntryPayload::from_bytes(br#"{"kind":"feed","occurred_at":1,"something_new":"hello"}"#)
                .unwrap();

        assert_eq!(decoded.occurred_at, 1);
    }

    #[test]
    fn a_payload_that_is_not_json_is_an_error_not_a_panic() {
        assert!(EntryPayload::from_bytes(b"not json at all").is_err());
        assert!(EntryPayload::from_bytes(&[0xff, 0x00, 0x13]).is_err());
        assert!(
            EntryPayload::from_bytes(br#"{"occurred_at":1}"#).is_err(),
            "kind is required"
        );
    }
}
