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

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::model::{
    Amount, AmountUnit, Baby, Consistency, DiaperType, Entry, EntryKind, FeedKind, Millis, Mood,
    PeeShade, PoopColor, SleepLocation, WakeMood,
};

/// Encode a payload for sealing. `what` names it in the error.
fn encode<T: Serialize>(value: &T, what: &str) -> Result<Vec<u8>> {
    serde_json::to_vec(value)
        .map_err(|error| CoreError::data(format!("could not encode {what}: {error}")))
}

/// Decode a payload that arrived in an event.
///
/// Unknown fields are ignored and missing ones default, so a device running a
/// newer build can add a field without an older one choking on it.
fn decode<T: DeserializeOwned>(bytes: &[u8], what: &str) -> Result<T> {
    serde_json::from_slice(bytes)
        .map_err(|error| CoreError::invalid(format!("{what} is not valid: {error}")))
}

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
    pub mood: Option<Mood>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub photo: Option<String>,
    /// The JPEG itself, hex-encoded, so a photo travels with its event.
    ///
    /// `photo` stays the local file name: the device that took the photo reads
    /// it from `photos/`, while a peer that received the entry reads these bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo_data: Option<String>,
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
            mood: entry.mood,
            note: entry.note.clone(),
            photo: entry.photo.clone(),
            photo_data: None,
        }
    }

    /// The authored half of an entry, carrying its photo bytes when this device
    /// has them.
    ///
    /// A device that received the entry over sync has no local file to read, so
    /// it passes `None`; the older event that carried the image still does.
    pub fn of_with_photo(entry: &Entry, photo: Option<&[u8]>) -> Self {
        let mut payload = Self::of(entry);
        payload.photo_data = photo.map(crate::crypto::to_hex);
        payload
    }

    /// Encode for sealing into an event.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        encode(self, "the entry")
    }

    /// Decode a payload that arrived in an event.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        decode(bytes, "entry payload")
    }
}

/// A baby's profile: its name, birthday and picture.
///
/// Deliberately not [`Baby`] — that also carries `theme`, which is a *device*
/// preference, not something one phone gets to set on another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BabyPayload {
    pub name: String,
    #[serde(default)]
    pub born: String,
    /// The picture's file name, so a peer can show that one exists.
    #[serde(default)]
    pub photo: Option<String>,
    /// The JPEG itself, hex-encoded, so the picture travels with the profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo_data: Option<String>,
}

impl BabyPayload {
    /// The authored half of a profile that already exists.
    pub fn of(baby: &Baby) -> Self {
        Self {
            name: baby.name.clone(),
            born: baby.born.clone(),
            photo: baby.photo.clone(),
            photo_data: None,
        }
    }

    /// The authored half of a profile, carrying its photo bytes when this device
    /// has them. A device that received the profile has no local file to read,
    /// so it passes `None`.
    pub fn of_with_photo(baby: &Baby, photo: Option<&[u8]>) -> Self {
        let mut payload = Self::of(baby);
        payload.photo_data = photo.map(crate::crypto::to_hex);
        payload
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        encode(self, "the baby")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        decode(bytes, "baby profile")
    }
}

#[cfg(test)]
mod entry_tests {
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
    fn a_mood_payload_round_trips() {
        let mut source = Entry::new(EntryKind::Mood, 1_700_000_000_000);
        source.mood = Some(Mood::Content);

        let payload = EntryPayload::of(&source);
        let decoded = EntryPayload::from_bytes(&payload.to_bytes().unwrap()).unwrap();

        assert_eq!(decoded.kind, EntryKind::Mood);
        assert_eq!(decoded.mood, Some(Mood::Content));
    }

    #[test]
    fn a_photo_rides_in_the_payload_as_hex() {
        let photo: &[u8] = b"\xff\xd8\xff not really a jpeg";
        let mut source = entry();
        source.photo = Some(format!("{}.jpg", source.id));

        let payload = EntryPayload::of_with_photo(&source, Some(photo));
        let decoded = EntryPayload::from_bytes(&payload.to_bytes().unwrap()).unwrap();

        assert_eq!(decoded.photo, source.photo, "the file name still travels");
        let encoded = decoded.photo_data.expect("the photo bytes travel");
        assert_eq!(crate::crypto::from_hex(&encoded).unwrap().as_slice(), photo);
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

#[cfg(test)]
mod baby_tests {
    use super::*;

    fn baby() -> Baby {
        Baby {
            id: Some("b1".into()),
            name: "Ada".into(),
            born: "2026-02-14".into(),
            photo: None,
        }
    }

    #[test]
    fn a_profile_round_trips_through_its_bytes() {
        let payload = BabyPayload::of(&baby());
        let decoded = BabyPayload::from_bytes(&payload.to_bytes().unwrap()).unwrap();

        assert_eq!(decoded.name, "Ada");
        assert_eq!(decoded.born, "2026-02-14");
    }

    #[test]
    fn a_profile_picture_rides_in_the_payload_as_hex() {
        let photo: &[u8] = b"\xff\xd8\xff not really a jpeg";
        let mut source = baby();
        source.photo = Some("baby-b1.jpg".into());

        let payload = BabyPayload::of_with_photo(&source, Some(photo));
        let decoded = BabyPayload::from_bytes(&payload.to_bytes().unwrap()).unwrap();

        assert_eq!(decoded.photo.as_deref(), Some("baby-b1.jpg"));
        let encoded = decoded.photo_data.expect("the photo bytes travel");
        assert_eq!(crate::crypto::from_hex(&encoded).unwrap().as_slice(), photo);
    }

    #[test]
    fn a_profile_carries_no_id_and_no_appearance() {
        // The event's `entry_id` is the baby's id; the theme is device-local and
        // lives in `device_settings`, so neither is authored here.
        let json = String::from_utf8(BabyPayload::of(&baby()).to_bytes().unwrap()).unwrap();

        assert!(!json.contains("\"id\""), "id comes from the event header");
        assert!(!json.contains("theme"), "appearance is not authored");
    }

    #[test]
    fn a_profile_needs_only_a_name() {
        let decoded = BabyPayload::from_bytes(br#"{"name":"Ada"}"#).unwrap();

        assert_eq!(decoded.name, "Ada");
        assert_eq!(decoded.born, "");
    }
}
