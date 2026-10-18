//! Validation rules shared by the app and the server. Everything that reaches
//! SQLite goes through here, so a buggy client can't corrupt the log.

use crate::error::{CoreError, Result};
use crate::model::{AmountUnit, Entry, EntryKind, FeedKind, Millis, Role, Theme};

/// Writes may arrive slightly "in the future" — allow for clock skew.
pub const FUTURE_SKEW_MS: Millis = 5 * 60 * 1000;
/// A breast feed longer than three hours is a typo, not a feed.
pub const MAX_BREAST_MINUTES: i64 = 180;
/// No bottle is bigger than two litres.
pub const MAX_BOTTLE_ML: i64 = 2000;
pub const MAX_NOTE_CHARS: usize = 500;

/// Current wall-clock time in milliseconds since the epoch.
pub fn now_millis() -> Millis {
    chrono::Utc::now().timestamp_millis()
}

/// Minutes this device is ahead of UTC — used for "today" boundaries.
/// (`$TZ` is set correctly on Android and iOS app processes.)
pub fn tz_offset_minutes() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

/// Validate an entry. Tombstones only need a valid id and kind.
pub fn validate_entry(entry: &Entry, now: Millis) -> Result<()> {
    validate_id(&entry.id)?;

    if let Some(note) = &entry.note {
        if note.chars().count() > MAX_NOTE_CHARS {
            return Err(CoreError::invalid(format!(
                "note is longer than {MAX_NOTE_CHARS} characters"
            )));
        }
    }

    if let Some(photo) = &entry.photo {
        validate_photo_name(photo, &entry.id)?;
    }

    // A tombstone is allowed to carry a bare payload: clients may purge a row
    // they only ever saw as deleted.
    if entry.deleted {
        return Ok(());
    }

    if entry.occurred_at > now + FUTURE_SKEW_MS {
        return Err(CoreError::invalid("occurred_at cannot be in the future"));
    }

    match entry.kind {
        EntryKind::Feed => validate_feed(entry),
        EntryKind::Sleep => validate_sleep(entry),
        EntryKind::Diaper => validate_diaper(entry),
    }
}

/// Convenience wrapper using the current clock.
pub fn validate_now(entry: &Entry) -> Result<()> {
    validate_entry(entry, now_millis())
}

fn validate_id(id: &str) -> Result<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| CoreError::invalid(format!("entry id \"{id}\" is not a UUID")))
}

/// Photos live at `<photos>/<entry id>.jpg`; anything else is a path-traversal
/// attempt and is rejected.
fn validate_photo_name(name: &str, entry_id: &str) -> Result<()> {
    let expected = format!("{entry_id}.jpg");
    if name != expected {
        return Err(CoreError::invalid(format!(
            "photo \"{name}\" must be \"{expected}\""
        )));
    }
    if name.contains(['/', '\\']) || name.contains("..") {
        return Err(CoreError::invalid("photo name must not contain a path"));
    }
    Ok(())
}

fn validate_feed(entry: &Entry) -> Result<()> {
    let Some(feed_kind) = entry.feed_kind else {
        return Err(CoreError::invalid("a feed needs a feed_kind"));
    };
    if entry.ended_at.is_some() {
        return Err(CoreError::invalid("a feed cannot have an end time"));
    }

    let amount = entry.amount;
    let unit = entry.amount_unit;

    match feed_kind {
        FeedKind::BreastLeft | FeedKind::BreastRight => {
            if let Some(unit) = unit {
                if unit != AmountUnit::Minutes {
                    return Err(CoreError::invalid("a breast feed is measured in minutes"));
                }
            }
            if let Some(minutes) = amount {
                if !(0..=MAX_BREAST_MINUTES).contains(&minutes) {
                    return Err(CoreError::invalid(format!(
                        "breast feed must be 0..={MAX_BREAST_MINUTES} minutes"
                    )));
                }
            }
        }
        FeedKind::Bottle => {
            if let Some(unit) = unit {
                if unit != AmountUnit::Milliliters {
                    return Err(CoreError::invalid("a bottle is measured in ml"));
                }
            }
            if let Some(ml) = amount {
                if !(0..=MAX_BOTTLE_ML).contains(&ml) {
                    return Err(CoreError::invalid(format!(
                        "bottle must be 0..={MAX_BOTTLE_ML} ml"
                    )));
                }
            }
        }
        FeedKind::Solids => {
            if let Some(ml) = amount {
                if !(0..=MAX_BOTTLE_ML).contains(&ml) {
                    return Err(CoreError::invalid(format!(
                        "solids must be 0..={MAX_BOTTLE_ML} ml"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_sleep(entry: &Entry) -> Result<()> {
    if let Some(end) = entry.ended_at {
        if end <= entry.occurred_at {
            return Err(CoreError::invalid("a sleep must end after it started"));
        }
    }
    Ok(())
}

fn validate_diaper(entry: &Entry) -> Result<()> {
    if entry.diaper_type.is_none() {
        return Err(CoreError::invalid("a diaper change needs a diaper_type"));
    }
    if entry.ended_at.is_some() {
        return Err(CoreError::invalid(
            "a diaper change cannot have an end time",
        ));
    }
    Ok(())
}

/// Roles and themes arrive from the UI as strings; reject anything unknown.
pub fn parse_role(value: &str) -> Result<Role> {
    Role::parse(value)
}

pub fn parse_theme(value: &str) -> Result<Theme> {
    Theme::parse(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Amount, Consistency, DiaperType, PeeShade, PoopColor, SleepLocation};

    fn feed(kind: FeedKind) -> Entry {
        let mut entry = Entry::new(EntryKind::Feed, now_millis());
        entry.feed_kind = Some(kind);
        entry
    }

    #[test]
    fn a_plain_breast_feed_is_valid() {
        let mut entry = feed(FeedKind::BreastLeft);
        entry.amount = Some(12);
        entry.amount_unit = Some(AmountUnit::Minutes);
        assert!(validate_now(&entry).is_ok());
    }

    #[test]
    fn a_feed_needs_a_kind() {
        let entry = Entry::new(EntryKind::Feed, now_millis());
        assert!(validate_now(&entry).is_err());
    }

    #[test]
    fn breast_feeds_are_measured_in_minutes() {
        let mut entry = feed(FeedKind::BreastRight);
        entry.amount = Some(90);
        entry.amount_unit = Some(AmountUnit::Milliliters);
        assert!(validate_now(&entry).is_err());
    }

    #[test]
    fn amounts_are_range_checked() {
        let mut entry = feed(FeedKind::Bottle);
        entry.amount = Some(5000);
        entry.amount_unit = Some(AmountUnit::Milliliters);
        assert!(validate_now(&entry).is_err());

        entry.amount = Some(90);
        assert!(validate_now(&entry).is_ok());

        let mut long = feed(FeedKind::BreastLeft);
        long.amount = Some(400);
        long.amount_unit = Some(AmountUnit::Minutes);
        assert!(validate_now(&long).is_err());
    }

    #[test]
    fn sleep_must_end_after_it_starts() {
        let mut entry = Entry::new(EntryKind::Sleep, 1_000);
        entry.ended_at = Some(500);
        assert!(validate_now(&entry).is_err());

        entry.ended_at = None; // still running — fine
        assert!(validate_now(&entry).is_ok());

        entry.ended_at = Some(2_000);
        assert!(validate_now(&entry).is_ok());
    }

    #[test]
    fn the_future_is_rejected_but_skew_is_allowed() {
        let now = now_millis();
        let mut entry = Entry::new(EntryKind::Sleep, now + 60_000);
        assert!(validate_entry(&entry, now).is_ok());

        entry.occurred_at = now + FUTURE_SKEW_MS + 1;
        assert!(validate_entry(&entry, now).is_err());
    }

    #[test]
    fn a_diaper_needs_a_type_and_full_enum_payload_is_valid() {
        let mut entry = Entry::new(EntryKind::Diaper, now_millis());
        assert!(validate_now(&entry).is_err());

        entry.diaper_type = Some(DiaperType::Both);
        entry.pee_shade = Some(PeeShade::Normal);
        entry.pee_amount = Some(Amount::Medium);
        entry.poop_color = Some(PoopColor::Mustard);
        entry.consistency = Some(Consistency::Soft);
        entry.poop_amount = Some(Amount::Small);
        entry.sleep_location = Some(SleepLocation::Crib); // ignored, not rejected
        assert!(validate_now(&entry).is_ok());
    }

    #[test]
    fn notes_are_length_limited() {
        let mut entry = Entry::new(EntryKind::Sleep, now_millis());
        entry.note = Some("x".repeat(MAX_NOTE_CHARS + 1));
        assert!(validate_now(&entry).is_err());
        entry.note = Some("x".repeat(MAX_NOTE_CHARS));
        assert!(validate_now(&entry).is_ok());
    }

    #[test]
    fn ids_must_be_uuids() {
        let mut entry = Entry::new(EntryKind::Sleep, now_millis());
        entry.id = "not-a-uuid".to_string();
        assert!(validate_now(&entry).is_err());
    }

    #[test]
    fn photo_names_cannot_escape_the_photos_directory() {
        let mut entry = Entry::new(EntryKind::Diaper, now_millis());
        entry.diaper_type = Some(DiaperType::Pee);

        entry.photo = Some("../../etc/passwd".to_string());
        assert!(validate_now(&entry).is_err());

        entry.photo = Some(format!("{}.jpg", entry.id));
        assert!(validate_now(&entry).is_ok());
    }

    #[test]
    fn tombstones_are_minimal_but_still_need_a_valid_id() {
        let mut entry = Entry::new(EntryKind::Feed, now_millis());
        entry.deleted = true;
        assert!(validate_now(&entry).is_ok());

        entry.id = "nope".to_string();
        assert!(validate_now(&entry).is_err());
    }
}
