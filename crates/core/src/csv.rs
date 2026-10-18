//! CSV export — the same five columns as the prototype:
//! `time,type,what,details,notes` (RFC 4180 quoting).

use crate::model::{Entry, Millis};

pub const HEADER: [&str; 5] = ["time", "type", "what", "details", "notes"];

/// Export every (non-deleted) entry, oldest first.
pub fn export(entries: &[Entry], now: Millis) -> String {
    let mut sorted: Vec<&Entry> = entries.iter().filter(|entry| !entry.deleted).collect();
    sorted.sort_by_key(|entry| (entry.occurred_at, entry.id.clone()));

    let mut out = HEADER.map(cell).join(",");
    out.push_str("\r\n");
    for entry in sorted {
        out.push_str(
            &[
                cell(&format_time(entry.occurred_at)),
                cell(entry.kind.as_str()),
                cell(&entry.title()),
                cell(&entry.details(now)),
                cell(entry.note.as_deref().unwrap_or("")),
            ]
            .join(","),
        );
        out.push_str("\r\n");
    }
    out
}

/// `2026-10-18T09:41:00Z` — UTC so exports are unambiguous.
pub fn format_time(ms: Millis) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// `1h 20m`, matching the UI's `dur()`.
pub fn format_duration(ms: i64) -> String {
    // Same rounding as the UI's dur(): Math.round(ms / 60000).
    let minutes = (ms.max(0) as f64 / 60_000.0).round() as i64;
    format!("{}h {}m", minutes / 60, minutes % 60)
}

/// RFC 4180 field: always quoted, inner quotes doubled.
fn cell(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        if ch == '"' {
            out.push('"');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Amount, AmountUnit, Consistency, DiaperType, Entry, EntryKind, FeedKind, PeeShade,
        PoopColor,
    };

    #[test]
    fn header_and_quoting_match_the_prototype() {
        let mut entry = Entry::new(EntryKind::Feed, 1_760_000_000_000);
        entry.feed_kind = Some(FeedKind::Bottle);
        entry.amount = Some(90);
        entry.amount_unit = Some(AmountUnit::Milliliters);
        entry.note = Some("gulped it, then \"burped\"\nall over".to_string());

        let csv = export(&[entry], 1_760_000_000_000);
        let mut lines = csv.lines();
        assert_eq!(
            lines.next().unwrap(),
            "\"time\",\"type\",\"what\",\"details\",\"notes\""
        );
        let row = lines.next().unwrap();
        assert!(row.starts_with("\"2025-10-09T"), "row was {row}");
        assert!(row.contains("\"feed\",\"Bottle\",\"90 ml\""));
        assert!(
            row.contains("\"gulped it, then \"\"burped\"\""),
            "row was {row}"
        );
    }

    #[test]
    fn entries_are_exported_oldest_first_and_tombstones_are_skipped() {
        let mut older = Entry::new(EntryKind::Sleep, 1_000);
        older.ended_at = Some(1_000 + 3_600_000);
        let mut newer = Entry::new(EntryKind::Diaper, 2_000);
        newer.diaper_type = Some(DiaperType::Pee);
        newer.pee_shade = Some(PeeShade::Pale);
        let mut removed = Entry::new(EntryKind::Sleep, 3_000);
        removed.deleted = true;

        let csv = export(&[newer.clone(), older.clone(), removed], 10_000);
        let rows: Vec<&str> = csv.lines().skip(1).collect();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains("\"Sleep\""));
        assert!(rows[0].contains("\"1h 0m\""));
        assert!(rows[1].contains("\"Pee\""));
        assert!(rows[1].contains("\"Pee: Pale\""));
    }

    #[test]
    fn durations_round_to_the_nearest_minute() {
        assert_eq!(format_duration(0), "0h 0m");
        assert_eq!(format_duration(89 * 60_000), "1h 29m");
        assert_eq!(format_duration(90 * 60_000 + 29_000), "1h 30m");
        assert_eq!(format_duration(-5), "0h 0m");
    }

    #[test]
    fn times_are_utc_rfc3339() {
        assert_eq!(format_time(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn diaper_details_combine_pee_and_poop() {
        let mut entry = Entry::new(EntryKind::Diaper, 1_000);
        entry.diaper_type = Some(DiaperType::Both);
        entry.pee_shade = Some(PeeShade::Normal);
        entry.pee_amount = Some(Amount::Large);
        entry.poop_color = Some(PoopColor::Green);
        entry.consistency = Some(Consistency::Runny);
        entry.poop_amount = Some(Amount::Small);
        assert_eq!(
            entry.details(1_000),
            "Pee: Normal · Large · Poop: Green, Runny, Small"
        );
    }
}
