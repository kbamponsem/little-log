//! The domain model. Serde ids are snake_case so stored/synced values never
//! depend on localised text — `label()` renders the prototype's wording.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Milliseconds since the Unix epoch, UTC.
pub type Millis = i64;

/// A fresh UUIDv7 string — the id every entry row is created with, so writes
/// from the app, the widget and the server all collide-proofly merge.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Declares an enum that round-trips through a stable snake_case id.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $id:literal, $label:literal);+ $(;)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub enum $name {
            $(#[serde(rename = $id)] $variant),+
        }

        impl $name {
            /// Every variant, in display order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The stable id stored in SQLite and sent over the wire.
            pub const fn as_str(self) -> &'static str {
                match self { $($name::$variant => $id),+ }
            }

            /// The human label from the prototype.
            pub const fn label(self) -> &'static str {
                match self { $($name::$variant => $label),+ }
            }

            /// Parse a stored id; unknown values are rejected.
            pub fn parse(value: &str) -> Result<Self> {
                match value {
                    $($id => Ok($name::$variant),)+
                    other => Err(CoreError::UnknownEnum {
                        enum_name: stringify!($name),
                        value: other.to_string(),
                    }),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = CoreError;
            fn from_str(value: &str) -> Result<Self> { Self::parse(value) }
        }
    };
}

string_enum!(EntryKind {
    Feed => "feed", "Feed";
    Sleep => "sleep", "Sleep";
    Diaper => "diaper", "Diaper"
});

string_enum!(
    /// What an event in the append-only log does.
    ///
    /// The ids are what the log stores and what signatures cover, so they can
    /// never change; the labels are for a future activity feed.
    EventKind {
        Create => "create", "Logged";
        Edit => "edit", "Edited";
        Delete => "delete", "Deleted";
        SleepStart => "sleep_start", "Asleep";
        SleepStop => "sleep_stop", "Awake";
        // A baby's profile. Its `entry_id` is the baby's id, so profiles ride
        // the same log and the same replication as entries.
        Profile => "profile", "Profile"
    }
);

string_enum!(FeedKind {
    BreastLeft => "breast_left", "Breast left";
    BreastRight => "breast_right", "Breast right";
    Bottle => "bottle", "Bottle";
    Solids => "solids", "Solids"
});

string_enum!(DiaperType {
    Pee => "pee", "Pee";
    Poop => "poop", "Poop";
    Both => "both", "Both"
});

string_enum!(PeeShade {
    Pale => "pale", "Pale";
    Normal => "normal", "Normal";
    Dark => "dark", "Dark"
});

string_enum!(Amount {
    Small => "small", "Small";
    Medium => "medium", "Medium";
    Large => "large", "Large"
});

string_enum!(PoopColor {
    Mustard => "mustard", "Mustard";
    Yellow => "yellow", "Yellow";
    Green => "green", "Green";
    Brown => "brown", "Brown";
    Dark => "dark", "Dark";
    Pale => "pale", "Pale";
    RedStreaked => "red_streaked", "Red-streaked"
});

string_enum!(Consistency {
    Liquid => "liquid", "Liquid";
    Runny => "runny", "Runny";
    Soft => "soft", "Soft";
    Pasty => "pasty", "Pasty";
    Firm => "firm", "Firm";
    Hard => "hard", "Hard"
});

string_enum!(SleepLocation {
    Crib => "crib", "Crib";
    Arms => "arms", "Arms";
    Stroller => "stroller", "Stroller";
    Car => "car", "Car";
    ParentsBed => "parents_bed", "Parents' bed"
});

string_enum!(WakeMood {
    Calm => "calm", "Calm";
    Fussy => "fussy", "Fussy";
    Cried => "cried", "Cried"
});

string_enum!(Role {
    Owner => "owner", "Owner";
    CoParent => "co_parent", "Co-parent";
    Caregiver => "caregiver", "Caregiver"
});

string_enum!(Theme {
    Auto => "auto", "Auto";
    Light => "light", "Light";
    Dark => "dark", "Dark"
});

string_enum!(
    /// Where a device's request to join a child's log has got to.
    ///
    /// The invite link deliberately carries no group key, so a joiner can reach
    /// us but cannot read a thing until a person on this side accepts. This is
    /// what the joiner polls on.
    JoinState {
        Pending => "pending", "Waiting to be accepted";
        Accepted => "accepted", "Accepted";
        Declined => "declined", "Declined"
    }
);

string_enum!(AmountUnit {
    Milliliters => "ml", "ml";
    Minutes => "min", "min"
});

impl PoopColor {
    /// Colours worth mentioning to a pediatrician.
    pub fn is_alert(self) -> bool {
        matches!(
            self,
            PoopColor::RedStreaked | PoopColor::Dark | PoopColor::Pale
        )
    }
}

impl AmountUnit {
    /// Tooltip-free label used in the amount stepper.
    pub const fn step(self) -> i64 {
        match self {
            AmountUnit::Milliliters => 10,
            AmountUnit::Minutes => 1,
        }
    }
}

/// One logged event. `updated_at` is the last-write-wins clock, `deleted` the
/// soft-delete tombstone — both drive sync.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(default)]
    pub baby_id: Option<String>,
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
    #[serde(default)]
    pub caregiver_id: Option<String>,
    pub created_at: Millis,
    pub updated_at: Millis,
    /// Soft-delete tombstone. Accepts `true`/`false` and `0`/`1` on the wire.
    #[serde(default, with = "bool01")]
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
            feed_kind: None,
            amount: None,
            amount_unit: None,
            diaper_type: None,
            pee_shade: None,
            pee_amount: None,
            poop_color: None,
            consistency: None,
            poop_amount: None,
            sleep_location: None,
            wake_mood: None,
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

    /// Sleep length so far (`now` used while a sleep is running).
    pub fn duration_ms(&self, now: Millis) -> i64 {
        self.ended_at.unwrap_or(now) - self.occurred_at
    }

    pub fn feed_label(&self) -> Option<&'static str> {
        self.feed_kind.map(|kind| kind.label())
    }

    /// The `what` column used by the CSV export and the entry rows.
    pub fn title(&self) -> String {
        match self.kind {
            EntryKind::Sleep => "Sleep".to_string(),
            EntryKind::Feed => self
                .feed_kind
                .map(|k| k.label().to_string())
                .unwrap_or_default(),
            EntryKind::Diaper => self
                .diaper_type
                .map(|t| t.label().to_string())
                .unwrap_or_default(),
        }
    }

    /// The `details` column used by the CSV export.
    pub fn details(&self, now: Millis) -> String {
        match self.kind {
            EntryKind::Sleep => crate::csv::format_duration(self.duration_ms(now)),
            EntryKind::Feed => match self.amount {
                Some(amount) => format!(
                    "{} {}",
                    amount,
                    self.amount_unit.map(|u| u.as_str()).unwrap_or("")
                ),
                None => String::new(),
            },
            EntryKind::Diaper => self.diaper_details(),
        }
    }

    fn diaper_details(&self) -> String {
        let mut parts = Vec::new();
        if self.diaper_type != Some(DiaperType::Poop) {
            let pee: Vec<&str> = [
                self.pee_shade.map(|s| s.label()),
                self.pee_amount.map(|a| a.label()),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !pee.is_empty() {
                parts.push(format!("Pee: {}", pee.join(" · ")));
            }
        }
        if self.diaper_type != Some(DiaperType::Pee) {
            let poop: Vec<&str> = [
                self.poop_color.map(|c| c.label()),
                self.consistency.map(|c| c.label()),
                self.poop_amount.map(|a| a.label()),
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

/// A baby's profile: its name and birthday.
///
/// There is no theme here on purpose. Appearance is a device preference and
/// lives in `device_settings`, so it never travels with the baby.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Baby {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    /// Birthday as `YYYY-MM-DD`, or empty.
    #[serde(default)]
    pub born: String,
}

impl Default for Baby {
    fn default() -> Self {
        Self {
            id: None,
            name: "Baby".to_string(),
            born: String::new(),
        }
    }
}

/// A person with access to the baby's log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Caregiver {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub baby_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    /// Argon2id PHC string. Never leaves the server.
    #[serde(default, skip_serializing)]
    pub password_hash: Option<String>,
    pub role: Role,
    #[serde(default)]
    pub created_at: Millis,
    #[serde(default)]
    pub updated_at: Millis,
}

impl Caregiver {
    pub fn new(name: impl Into<String>, role: Role) -> Self {
        let now = crate::validate::now_millis();
        Self {
            id: Some(uuid::Uuid::now_v7().to_string()),
            baby_id: None,
            name: name.into(),
            email: None,
            password_hash: None,
            role,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Serialise/de-serialise a boolean as `true`/`false` or `0`/`1`.
mod bool01 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &bool, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(i64::from(*value))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Bool01 {
            Bool(bool),
            Int(i64),
            Text(String),
        }

        Ok(match Bool01::deserialize(deserializer)? {
            Bool01::Bool(value) => value,
            Bool01::Int(value) => value != 0,
            Bool01::Text(value) => matches!(value.trim(), "1" | "true"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_ids_round_trip() {
        for kind in FeedKind::ALL {
            assert_eq!(FeedKind::parse(kind.as_str()).unwrap(), *kind);
        }
        for color in PoopColor::ALL {
            assert_eq!(PoopColor::parse(color.as_str()).unwrap(), *color);
        }
        for role in Role::ALL {
            assert_eq!(Role::parse(role.as_str()).unwrap(), *role);
        }
    }

    #[test]
    fn enum_ids_are_snake_case_and_match_labels() {
        assert_eq!(FeedKind::BreastLeft.as_str(), "breast_left");
        assert_eq!(FeedKind::BreastLeft.label(), "Breast left");
        assert_eq!(PoopColor::RedStreaked.as_str(), "red_streaked");
        assert_eq!(SleepLocation::ParentsBed.as_str(), "parents_bed");
        assert_eq!(Role::CoParent.as_str(), "co_parent");
    }

    #[test]
    fn unknown_enum_values_are_rejected() {
        assert!(FeedKind::parse("breast_up").is_err());
        assert!(PoopColor::parse("rainbow").is_err());
        assert!(Role::parse("admin").is_err());
    }

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
        assert_eq!(entry.feed_kind, Some(FeedKind::BreastLeft));
        assert_eq!(entry.amount_unit, Some(AmountUnit::Minutes));
        assert!(!entry.deleted);
        assert!(entry.baby_id.is_none());

        let round_tripped: Entry =
            serde_json::from_str(&serde_json::to_string(&entry).unwrap()).unwrap();
        assert_eq!(round_tripped, entry);
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
        feed.feed_kind = Some(FeedKind::Bottle);
        feed.amount = Some(90);
        feed.amount_unit = Some(AmountUnit::Milliliters);
        assert_eq!(feed.title(), "Bottle");
        assert_eq!(feed.details(0), "90 ml");

        let mut diaper = Entry::new(EntryKind::Diaper, 0);
        diaper.diaper_type = Some(DiaperType::Both);
        diaper.pee_shade = Some(PeeShade::Normal);
        diaper.pee_amount = Some(Amount::Medium);
        diaper.poop_color = Some(PoopColor::Mustard);
        diaper.consistency = Some(Consistency::Soft);
        assert_eq!(diaper.title(), "Both");
        assert_eq!(
            diaper.details(0),
            "Pee: Normal · Medium · Poop: Mustard, Soft"
        );
    }

    #[test]
    fn alert_colors_match_the_trends_warning() {
        assert!(PoopColor::RedStreaked.is_alert());
        assert!(PoopColor::Dark.is_alert());
        assert!(PoopColor::Pale.is_alert());
        assert!(!PoopColor::Mustard.is_alert());
    }
}
