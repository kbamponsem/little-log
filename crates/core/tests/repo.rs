//! Repository behaviour: last-write-wins, tombstones, cursor paging, invites.

use little_log_core::model::{
    AmountUnit, Baby, Caregiver, DiaperType, Entry, EntryKind, FeedKind, Role, Theme,
};
use little_log_core::sync::Cursor;
use little_log_core::{validate, Repo};

fn feed(repo: &Repo, updated_at: i64) -> Entry {
    let mut entry = Entry::new(EntryKind::Feed, updated_at);
    entry.updated_at = updated_at;
    entry.feed_kind = Some(FeedKind::Bottle);
    entry.amount = Some(90);
    entry.amount_unit = Some(AmountUnit::Milliliters);
    repo.save_entry(&entry).unwrap();
    entry
}

#[test]
fn first_run_creates_the_baby_once() {
    let repo = Repo::open_in_memory().unwrap();
    assert!(repo.baby().unwrap().is_none());

    let baby = repo.ensure_baby("Ada").unwrap();
    assert_eq!(baby.name, "Ada");
    assert!(baby.id.is_some());

    let again = repo.ensure_baby("ignored").unwrap();
    assert_eq!(again, baby);
}

#[test]
fn the_baby_profile_is_editable() {
    let repo = Repo::open_in_memory().unwrap();
    let mut baby = repo.ensure_baby("Baby").unwrap();
    baby.name = "Ada".into();
    baby.born = "2026-03-02".into();
    baby.theme = Theme::Dark;

    let saved = repo.save_baby(&baby).unwrap();
    assert_eq!(saved.name, "Ada");
    assert_eq!(saved.born, "2026-03-02");
    assert_eq!(saved.theme, Theme::Dark);
    assert_eq!(repo.baby().unwrap().unwrap().theme, Theme::Dark);
}

#[test]
fn entries_are_listed_newest_first() {
    let repo = Repo::open_in_memory().unwrap();
    let older = feed(&repo, 1_000);
    let newer = feed(&repo, 2_000);

    let listed = repo.entries().unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec![newer.id.as_str(), older.id.as_str()]
    );
    assert_eq!(repo.count_entries().unwrap(), 2);
}

#[test]
fn newer_writes_win_and_stale_writes_are_ignored() {
    let repo = Repo::open_in_memory().unwrap();
    let stored = feed(&repo, 1_000);

    let mut stale = stored.clone();
    stale.note = Some("stale".into());
    stale.updated_at = 900;
    assert!(!repo.save_entry(&stale).unwrap());
    assert_eq!(repo.entry(&stored.id).unwrap().unwrap().note, None);

    let mut fresh = stored.clone();
    fresh.note = Some("fresh".into());
    fresh.updated_at = 2_000;
    assert!(repo.save_entry(&fresh).unwrap());
    assert_eq!(
        repo.entry(&stored.id).unwrap().unwrap().note.as_deref(),
        Some("fresh")
    );
}

#[test]
fn re_sending_the_same_push_changes_nothing() {
    let repo = Repo::open_in_memory().unwrap();
    let entry = feed(&repo, 1_000);
    repo.save_entry(&entry).unwrap();

    let before = repo.entries().unwrap();
    assert_eq!(repo.count_entries().unwrap(), 1);
    assert_eq!(repo.entries().unwrap(), before);
}

#[test]
fn local_writes_are_validated_but_remote_clock_skew_is_tolerated() {
    let repo = Repo::open_in_memory().unwrap();
    let now = validate::now_millis();

    let mut too_big = Entry::new(EntryKind::Feed, now);
    too_big.feed_kind = Some(FeedKind::Bottle);
    too_big.amount = Some(5_000);
    too_big.amount_unit = Some(AmountUnit::Milliliters);
    assert!(repo.save_entry(&too_big).is_err());

    // A device whose clock runs ahead can still push.
    let mut ahead = Entry::new(EntryKind::Sleep, now + 60 * 60 * 1000);
    ahead.updated_at = now + 60 * 60 * 1000;
    assert!(repo.save_remote_entry(&ahead).unwrap());
}

#[test]
fn deleting_soft_deletes_and_the_tombstone_wins() {
    let repo = Repo::open_in_memory().unwrap();
    let entry = feed(&repo, 1_000);

    let tombstone = repo.soft_delete(&entry.id, Some("caregiver-1")).unwrap();
    assert!(tombstone.deleted);
    assert_eq!(tombstone.caregiver_id.as_deref(), Some("caregiver-1"));
    assert!(repo.entries().unwrap().is_empty());
    assert_eq!(repo.count_entries().unwrap(), 0);

    // The tombstone still syncs…
    let changed = repo.entries_since(None).unwrap();
    assert_eq!(changed.len(), 1);
    assert!(changed[0].deleted);

    // …and an older edit cannot resurrect the entry.
    let mut resurrect = entry.clone();
    resurrect.updated_at = 500;
    assert!(!repo.save_entry(&resurrect).unwrap());
    assert!(repo.entries().unwrap().is_empty());
}

#[test]
fn delete_everything_tombstones_the_whole_log() {
    let repo = Repo::open_in_memory().unwrap();
    feed(&repo, 1_000);
    feed(&repo, 2_000);
    let mut diaper = Entry::new(EntryKind::Diaper, 3_000);
    diaper.diaper_type = Some(DiaperType::Pee);
    repo.save_entry(&diaper).unwrap();

    assert_eq!(repo.delete_all(None).unwrap(), 3);
    assert_eq!(repo.count_entries().unwrap(), 0);
    assert_eq!(repo.entries_since(None).unwrap().len(), 3);
    assert_eq!(repo.delete_all(None).unwrap(), 0);
}

#[test]
fn cursor_paging_never_loses_rows_with_the_same_timestamp() {
    let repo = Repo::open_in_memory().unwrap();
    // Two rows sharing an updated_at, distinguishable only by id.
    let mut first = Entry::new(EntryKind::Feed, 5_000);
    first.id = "00000000-0000-7000-8000-000000000001".into();
    first.feed_kind = Some(FeedKind::Bottle);
    first.updated_at = 5_000;
    let mut second = Entry::new(EntryKind::Feed, 5_000);
    second.id = "00000000-0000-7000-8000-000000000002".into();
    second.feed_kind = Some(FeedKind::Bottle);
    second.updated_at = 5_000;
    repo.force_entry(&first).unwrap();
    repo.force_entry(&second).unwrap();

    let page = repo.entries_since(None).unwrap();
    assert_eq!(page.len(), 2);
    let cursor = Cursor::latest(&page).unwrap();
    assert_eq!(repo.entries_since(Some(&cursor)).unwrap().len(), 0);

    // A cursor sitting on the *first* row still returns the second.
    let mid = Cursor::of(&page[0]);
    let rest = repo.entries_since(Some(&mid)).unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].id, page[1].id);
    assert_eq!(repo.latest_cursor().unwrap(), Some(cursor));
}

#[test]
fn caregivers_and_passwords_are_stored_locally() {
    let repo = Repo::open_in_memory().unwrap();
    let baby = repo.ensure_baby("Ada").unwrap();

    let mut owner = Caregiver::new("Kwabena", Role::Owner);
    owner.baby_id = baby.id.clone();
    owner.email = Some("kwabena@example.com".into());
    owner.password_hash = Some("$argon2id$v=19$m=19456,t=2,p=1$hash".into());
    repo.save_caregiver(&owner).unwrap();

    let caregivers = repo.caregivers().unwrap();
    assert_eq!(caregivers.len(), 1);
    assert_eq!(caregivers[0].role, Role::Owner);
    // The hash never leaves the repository.
    assert_eq!(caregivers[0].password_hash, None);

    let (hash, caregiver) = repo
        .password_hash_for("kwabena@example.com")
        .unwrap()
        .unwrap();
    assert!(hash.starts_with("$argon2id$"));
    assert_eq!(caregiver.name, "Kwabena");
    assert!(repo
        .password_hash_for("nobody@example.com")
        .unwrap()
        .is_none());
}

#[test]
fn invite_links_are_single_use_and_expire() {
    let repo = Repo::open_in_memory().unwrap();
    let now = validate::now_millis();

    repo.create_invite("hash-1", Role::Caregiver, "owner-1", now + 60_000)
        .unwrap();
    let invite = repo.invite("hash-1").unwrap().unwrap();
    assert_eq!(invite.role, Role::Caregiver);
    assert!(!invite.is_redeemed());

    assert_eq!(
        repo.accept_invite("hash-1", "grandma").unwrap(),
        Role::Caregiver
    );
    // Second redemption fails.
    assert!(repo.accept_invite("hash-1", "someone-else").is_err());

    repo.create_invite("hash-2", Role::CoParent, "owner-1", now - 1)
        .unwrap();
    let expired = repo.invite("hash-2").unwrap().unwrap();
    assert!(expired.is_expired(now));
    assert!(repo.accept_invite("hash-2", "partner").is_err());

    assert_eq!(repo.prune_invites().unwrap(), 2);
    assert!(repo.invite("hash-1").unwrap().is_none());
}

#[test]
fn sessions_resolve_to_a_caregiver_and_expire() {
    let repo = Repo::open_in_memory().unwrap();
    let owner = repo
        .save_caregiver(&Caregiver::new("Kwabena", Role::Owner))
        .unwrap();
    let owner_id = owner.id.clone().unwrap();
    let now = validate::now_millis();

    repo.create_session("token-hash", &owner_id, now + 60_000)
        .unwrap();
    assert_eq!(
        repo.session_caregiver("token-hash").unwrap().unwrap().id,
        Some(owner_id.clone())
    );

    repo.create_session("expired", &owner_id, now - 1).unwrap();
    assert!(repo.session_caregiver("expired").unwrap().is_none());

    repo.delete_session("token-hash").unwrap();
    assert!(repo.session_caregiver("token-hash").unwrap().is_none());
    assert_eq!(repo.prune_sessions().unwrap(), 1);
}

#[test]
fn the_schema_migrates_only_once() {
    let dir = std::env::temp_dir().join(format!("little-log-{}", validate::now_millis()));
    let path = dir.join("little-log.sqlite3");

    {
        let repo = Repo::open(&path).unwrap();
        repo.ensure_baby("Ada").unwrap();
    }
    {
        // Reopening must not wipe or re-create anything.
        let repo = Repo::open(&path).unwrap();
        assert_eq!(repo.baby().unwrap().unwrap().name, "Ada");
        let version: i64 = repo
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, little_log_core::repo::SCHEMA_VERSION);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_entry_cannot_be_deleted_twice_over() {
    let repo = Repo::open_in_memory().unwrap();
    let entry = feed(&repo, 1_000);
    repo.soft_delete(&entry.id, None).unwrap();
    // Soft-deleting again is allowed (idempotent tombstone)…
    let again = repo.soft_delete(&entry.id, None).unwrap();
    assert!(again.deleted);
    // …but an unknown id errors.
    assert!(repo
        .soft_delete("00000000-0000-7000-8000-0000000000ff", None)
        .is_err());
}

#[test]
fn the_baby_id_and_author_can_be_filled_in_by_the_caller() {
    let repo = Repo::open_in_memory().unwrap();
    let baby = repo.ensure_baby("Ada").unwrap();

    let mut entry = Entry::new(EntryKind::Diaper, validate::now_millis());
    entry.diaper_type = Some(DiaperType::Pee);
    entry.baby_id = baby.id.clone();
    entry.caregiver_id = Some("caregiver-1".into());
    repo.save_entry(&entry).unwrap();

    let stored = repo.entry(&entry.id).unwrap().unwrap();
    assert_eq!(stored.baby_id, baby.id);
    assert_eq!(stored.caregiver_id.as_deref(), Some("caregiver-1"));
}

#[test]
fn csv_export_round_trips_from_the_repository() {
    let repo = Repo::open_in_memory().unwrap();
    let mut sleep = Entry::new(EntryKind::Sleep, 1_000);
    sleep.ended_at = Some(1_000 + 80 * 60_000);
    repo.save_entry(&sleep).unwrap();
    let mut diaper = Entry::new(EntryKind::Diaper, 2_000);
    diaper.diaper_type = Some(DiaperType::Both);
    diaper.note = Some("blowout".into());
    repo.save_entry(&diaper).unwrap();

    let csv = little_log_core::csv::export(&repo.entries().unwrap(), validate::now_millis());
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(rows.len(), 3);
    assert!(rows[1].contains("\"1h 20m\""));
    assert!(rows[2].contains("\"blowout\""));
    assert!(csv.ends_with("\r\n"));
}

#[test]
fn saving_a_baby_without_an_id_updates_the_existing_one() {
    let repo = Repo::open_in_memory().unwrap();
    repo.ensure_baby("Baby").unwrap();

    // What a client that only knows the profile fields sends.
    let patch = Baby {
        id: None,
        name: "Ada".into(),
        born: "2026-03-02".into(),
        theme: Theme::Dark,
    };
    let saved = repo.save_baby(&patch).unwrap();
    assert_eq!(saved.name, "Ada");

    // Still exactly one baby, and it is the updated row.
    let babies: i64 = repo
        .connection()
        .query_row("SELECT COUNT(*) FROM babies", [], |row| row.get(0))
        .unwrap();
    assert_eq!(babies, 1);
    assert_eq!(repo.baby().unwrap().unwrap().name, "Ada");
}

#[test]
fn a_baby_with_a_blank_name_is_still_a_baby() {
    let repo = Repo::open_in_memory().unwrap();
    let baby = Baby::default();
    let saved = repo.save_baby(&baby).unwrap();
    assert_eq!(saved.name, "Baby");
    assert_eq!(saved.theme, Theme::Auto);
}
