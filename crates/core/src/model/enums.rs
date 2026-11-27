//! Stable value types. Each enum's id is what SQLite and the wire store, so the
//! ids can never change; the labels are for rendering.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

string_enum!(EntryKind {
    Feed => "feed", "Feed";
    Sleep => "sleep", "Sleep";
    Diaper => "diaper", "Diaper";
    Mood => "mood", "Mood"
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

string_enum!(
    /// How the baby felt overall across a day, logged as a mood check-in.
    Mood {
        Happy => "happy", "Happy";
        Content => "content", "Content";
        Fussy => "fussy", "Fussy";
        Unwell => "unwell", "Unwell";
        HardDay => "hard_day", "Hard day"
    }
);

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
