//! A logged entry: the shared columns, the per-kind groups, and the local
//! rendering helpers (titles, durations, CSV details).

use serde::{Deserialize, Serialize};

use super::{
    Amount, AmountUnit, Consistency, DiaperType, EntryKind, FeedKind, Millis, Mood, PeeShade,
    PoopColor, SleepLocation, WakeMood,
};

/// The fields that belong to a feed, and to nothing else.
///
/// Flattened into [`Entry`], so the stored columns and the JSON keys are still
/// `feed_kind`, `amount` and `amount_unit`; only the Rust shape is grouped.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedDetails {
    #[serde(default, rename = "feed_kind")]
    pub kind: Option<FeedKind>,
    #[serde(default)]
    pub amount: Option<i64>,
    #[serde(default)]
    pub amount_unit: Option<AmountUnit>,
}

/// The fields that belong to a diaper change, and to nothing else.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiaperDetails {
    #[serde(default, rename = "diaper_type")]
    pub kind: Option<DiaperType>,
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
}

/// The fields that belong to a sleep, and to nothing else.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SleepDetails {
    #[serde(default, rename = "sleep_location")]
    pub location: Option<SleepLocation>,
    #[serde(default)]
    pub wake_mood: Option<WakeMood>,
}

/// One logged event.
///
/// The shared columns stay at this level; each kind's nullable columns live in
/// one of the groups below. `#[serde(flatten)]` is what keeps the storage model
/// untouched: SQLite still sees one flat row and the JSON still has the same
/// top-level keys, so the frontend contract does not move.
///
/// `updated_at` is the last-write-wins clock, `deleted` the soft-delete
/// tombstone — both drive sync.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(default)]
    pub baby_id: Option<String>,
    pub kind: EntryKind,
    pub occurred_at: Millis,
    #[serde(default)]
    pub ended_at: Option<Millis>,
    #[serde(flatten)]
    pub feed: FeedDetails,
    #[serde(flatten)]
    pub diaper: DiaperDetails,
    #[serde(flatten)]
    pub sleep: SleepDetails,
    /// How the baby felt overall during the day, for a `mood` entry.
    #[serde(default)]
    pub mood: Option<Mood>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub photo: Option<String>,
    #[serde(default)]
    pub caregiver_id: Option<String>,
    pub created_at: Millis,
    pub updated_at: Millis,
    /// Soft-delete tombstone. Accepts `true`/`false` and `0`/`1` on the wire.
    #[serde(default, with = "crate::bool01")]
    pub deleted: bool,
}

impl Entry {
    /// A new entry with a device-generated UUIDv7 and every optional field empty.
    pub fn new(kind: EntryKind, occurred_at: Millis) -> Self {
        let now = crate::validate::now_millis();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            baby_id: None,
            kind,
            occurred_at,
            ended_at: None,
            feed: FeedDetails::default(),
            diaper: DiaperDetails::default(),
            sleep: SleepDetails::default(),
            mood: None,
            note: None,
            photo: None,
            caregiver_id: None,
            created_at: now,
            updated_at: now,
            deleted: false,
        }
    }

    pub fn is_sleeping(&self) -> bool {
        self.kind == EntryKind::Sleep && self.ended_at.is_none() && !self.deleted
    }

    /// A feed that has started and not been stopped yet.
    pub fn is_feeding(&self) -> bool {
        self.kind == EntryKind::Feed && self.ended_at.is_none() && !self.deleted
    }

    /// Sleep length so far (`now` used while a sleep is running).
    pub fn duration_ms(&self, now: Millis) -> i64 {
        self.ended_at.unwrap_or(now) - self.occurred_at
    }

    pub fn feed_label(&self) -> Option<&'static str> {
        self.feed.kind.map(|kind| kind.label())
    }

    /// The `what` column used by the CSV export and the entry rows.
    pub fn title(&self) -> String {
        match self.kind {
            EntryKind::Sleep => "Sleep".to_string(),
            EntryKind::Feed => self
                .feed
                .kind
                .map(|k| k.label().to_string())
                .unwrap_or_default(),
            EntryKind::Diaper => self
                .diaper
                .kind
                .map(|t| t.label().to_string())
                .unwrap_or_default(),
            EntryKind::Mood => self
                .mood
                .map(|mood| mood.label().to_string())
                .unwrap_or_default(),
        }
    }

    /// The `details` column used by the CSV export.
    pub fn details(&self, now: Millis) -> String {
        match self.kind {
            EntryKind::Sleep => crate::csv::format_duration(self.duration_ms(now)),
            EntryKind::Feed => match self.feed.amount {
                Some(amount) => format!(
                    "{} {}",
                    amount,
                    self.feed.amount_unit.map(|u| u.as_str()).unwrap_or("")
                ),
                None if self.ended_at.is_some_and(|end| end > self.occurred_at) => {
                    crate::csv::format_duration(self.duration_ms(now))
                }
                None => String::new(),
            },
            EntryKind::Diaper => self.diaper_details(),
            EntryKind::Mood => String::new(),
        }
    }

    fn diaper_details(&self) -> String {
        let diaper = &self.diaper;
        let mut parts = Vec::new();
        if diaper.kind != Some(DiaperType::Poop) {
            let pee: Vec<&str> = [
                diaper.pee_shade.map(|s| s.label()),
                diaper.pee_amount.map(|a| a.label()),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !pee.is_empty() {
                parts.push(format!("Pee: {}", pee.join(" · ")));
            }
        }
        if diaper.kind != Some(DiaperType::Pee) {
            let poop: Vec<&str> = [
                diaper.poop_color.map(|c| c.label()),
                diaper.consistency.map(|c| c.label()),
                diaper.poop_amount.map(|a| a.label()),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !poop.is_empty() {
                parts.push(format!("Poop: {}", poop.join(", ")));
            }
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_json_matches_the_frontend_contract() {
        let json = r#"{
            "id": "0192f0a1-2b3c-7d4e-8f5a-6b7c8d9e0f10",
            "kind": "feed",
            "occurred_at": 1760000000000,
            "ended_at": null,
            "feed_kind": "breast_left",
            "amount": 10,
            "amount_unit": "min",
            "note": null,
            "deleted": 0,
            "created_at": 1760000000000,
            "updated_at": 1760000000000
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.kind, EntryKind::Feed);
        assert_eq!(entry.feed.kind, Some(FeedKind::BreastLeft));
        assert_eq!(entry.feed.amount_unit, Some(AmountUnit::Minutes));
        assert!(!entry.deleted);
        assert!(entry.baby_id.is_none());

        let round_tripped: Entry =
            serde_json::from_str(&serde_json::to_string(&entry).unwrap()).unwrap();
        assert_eq!(round_tripped, entry);

        // The groups are a Rust-side split only: `flatten` must keep the wire
        // shape identical to the flat columns the frontend and old builds use.
        let value = serde_json::to_value(&entry).unwrap();
        for group in ["feed", "diaper", "sleep"] {
            assert!(value.get(group).is_none(), "{group} must stay flattened");
        }
        assert_eq!(value["feed_kind"], "breast_left");
        assert_eq!(value["amount"], 10);
        assert_eq!(value["amount_unit"], "min");
    }

    #[test]
    fn deleted_accepts_bool_and_int() {
        let entry: Entry = serde_json::from_str(r#"{"id":"x","kind":"sleep","occurred_at":1,"created_at":1,"updated_at":1,"deleted":true}"#).unwrap();
        assert!(entry.deleted);
        let entry: Entry = serde_json::from_str(r#"{"id":"x","kind":"sleep","occurred_at":1,"created_at":1,"updated_at":1,"deleted":1}"#).unwrap();
        assert!(entry.deleted);
    }

    #[test]
    fn titles_and_details_follow_the_prototype() {
        let mut sleep = Entry::new(EntryKind::Sleep, 0);
        sleep.ended_at = Some(90 * 60 * 1000);
        assert_eq!(sleep.title(), "Sleep");
        assert_eq!(sleep.details(0), "1h 30m");

        let mut feed = Entry::new(EntryKind::Feed, 0);
        feed.feed.kind = Some(FeedKind::Bottle);
        feed.feed.amount = Some(90);
        feed.feed.amount_unit = Some(AmountUnit::Milliliters);
        assert_eq!(feed.title(), "Bottle");
        assert_eq!(feed.details(0), "90 ml");

        // A started-and-stopped feed has no typed amount, so the duration is
        // what the export shows.
        let mut timed = Entry::new(EntryKind::Feed, 0);
        timed.feed.kind = Some(FeedKind::BreastLeft);
        timed.ended_at = Some(20 * 60 * 1000);
        assert_eq!(timed.details(0), "0h 20m");

        let mut diaper = Entry::new(EntryKind::Diaper, 0);
        diaper.diaper.kind = Some(DiaperType::Both);
        diaper.diaper.pee_shade = Some(PeeShade::Normal);
        diaper.diaper.pee_amount = Some(Amount::Medium);
        diaper.diaper.poop_color = Some(PoopColor::Mustard);
        diaper.diaper.consistency = Some(Consistency::Soft);
        assert_eq!(diaper.title(), "Both");
        assert_eq!(
            diaper.details(0),
            "Pee: Normal · Medium · Poop: Mustard, Soft"
        );

        let mut mood = Entry::new(EntryKind::Mood, 0);
        mood.mood = Some(Mood::HardDay);
        assert_eq!(mood.title(), "Hard day");
        assert_eq!(mood.details(0), "");
    }

    #[test]
    fn a_feed_without_an_end_is_still_running() {
        let mut feed = Entry::new(EntryKind::Feed, 0);
        feed.feed.kind = Some(FeedKind::BreastLeft);
        assert!(feed.is_feeding());

        feed.ended_at = Some(1);
        assert!(!feed.is_feeding());

        feed.ended_at = None;
        feed.deleted = true;
        assert!(!feed.is_feeding());
    }
}
