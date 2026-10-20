//! Authoring: the only way a change enters the log.
//!
//! A row is never written directly. A change is authored into a signed event,
//! appended to the log, and folded back into the row. That is what makes the
//! projection and the log provably agree, including after a restart.
//!
//! The clock is seeded from the log rather than from zero, because the HLC is
//! the whole ordering rule: a device whose clock restarted at boot could author
//! writes that sort before events it already holds.

use crate::clock::Clock;
use crate::crypto::{DeviceKey, GroupKey};
use crate::error::{CoreError, Result};
use crate::event::Event;
use crate::model::{new_id, Baby, Entry, EventKind};
use crate::payload::{BabyPayload, EntryPayload};
use crate::repo::Repo;
use crate::validate::now_millis;

impl Repo {
    /// A clock for this device, observing the newest event already in the log.
    pub fn clock(&self, node: impl Into<String>) -> Result<Clock> {
        let mut clock = Clock::new(node);
        if let Some(newest) = self.log().heads()?.into_iter().max() {
            clock.observe(&newest, now_millis());
        }
        Ok(clock)
    }

    /* -------------------------------------------------------------- profile */

    /// Create a baby: a fresh id, a fresh group key, and a first profile event.
    ///
    /// A baby *is* a gossip topic, so whoever creates it generates the key that
    /// seals everything about it and hands that key out when inviting a device.
    pub fn create_baby(
        &self,
        key: &DeviceKey,
        clock: &mut Clock,
        name: &str,
        born: &str,
    ) -> Result<Baby> {
        let id = new_id();
        let group = GroupKey::generate()?;
        self.save_group_key(&id, &group)?;

        let baby = Baby {
            id: Some(id.clone()),
            name: name.to_string(),
            born: born.to_string(),
            photo: None,
        };
        self.record_profile(key, clock, &group, &baby)?;

        self.baby_by_id(&id)?
            .ok_or_else(|| CoreError::data("baby missing after its profile event"))
    }

    /// Author a profile write for a baby this device holds the key for.
    pub fn record_profile(
        &self,
        key: &DeviceKey,
        clock: &mut Clock,
        group: &GroupKey,
        baby: &Baby,
    ) -> Result<Event> {
        self.record_profile_with_photo(key, clock, group, baby, None)
    }

    /// [`Self::record_profile`], carrying the picture bytes in the event so the
    /// image reaches a peer along with the name and birthday.
    pub fn record_profile_with_photo(
        &self,
        key: &DeviceKey,
        clock: &mut Clock,
        group: &GroupKey,
        baby: &Baby,
        photo: Option<&[u8]>,
    ) -> Result<Event> {
        let Some(id) = baby.id.clone() else {
            return Err(CoreError::invalid("a profile needs a baby id"));
        };
        let event = seal_profile(key, clock, group, &id, baby, photo)?;
        self.log().insert(&event)?;
        self.project_baby(&id, group)?;
        Ok(event)
    }

    /// Save a baby's name, birthday and picture as a new profile write.
    pub fn save_profile(&self, key: &DeviceKey, clock: &mut Clock, baby: &Baby) -> Result<Baby> {
        self.save_profile_with_photo(key, clock, baby, None)
    }

    /// [`Self::save_profile`], carrying the picture bytes in the event.
    pub fn save_profile_with_photo(
        &self,
        key: &DeviceKey,
        clock: &mut Clock,
        baby: &Baby,
        photo: Option<&[u8]>,
    ) -> Result<Baby> {
        let Some(id) = baby.id.clone() else {
            return Err(CoreError::invalid("a profile needs a baby id"));
        };
        let group = self
            .group_key(&id)?
            .ok_or_else(|| CoreError::invalid("this device does not hold that baby's key"))?;
        self.record_profile_with_photo(key, clock, &group, baby, photo)?;
        self.baby_by_id(&id)?
            .ok_or_else(|| CoreError::data("baby missing after save"))
    }
}

impl Repo {
    /* ---------------------------------------------------------------- entries */

    /// Record a change to an entry: author the event, append it, fold the row.
    ///
    /// This is the app's write path. The row is not written directly any more —
    /// it is the *result* of the event, which is what makes the local view and
    /// what a peer receives the same thing by construction.
    ///
    /// `kind` distinguishes a first write from a later one for the reader's sake;
    /// the fold treats every payload-bearing kind alike.
    pub fn record_entry(
        &self,
        key: &DeviceKey,
        clock: &mut Clock,
        entry: &Entry,
        kind: EventKind,
    ) -> Result<Entry> {
        self.record_entry_with_photo(key, clock, entry, kind, None)
    }

    /// [`Self::record_entry`], carrying the photo bytes in the event so the
    /// image reaches a peer along with the entry.
    pub fn record_entry_with_photo(
        &self,
        key: &DeviceKey,
        clock: &mut Clock,
        entry: &Entry,
        kind: EventKind,
        photo: Option<&[u8]>,
    ) -> Result<Entry> {
        let Some(topic) = entry.baby_id.clone() else {
            return Err(CoreError::invalid("an entry needs a child to belong to"));
        };
        let group = self
            .group_key(&topic)?
            .ok_or_else(|| CoreError::invalid("this device does not hold that child's key"))?;
        let payload = EntryPayload::of_with_photo(entry, photo).to_bytes()?;

        let event = Event::seal(
            key,
            &group,
            &topic,
            &entry.id,
            kind,
            clock.tick(now_millis()),
            &payload,
        )?;
        self.log().insert(&event)?;
        self.project_entry(&entry.id, &group)?;

        self.entry(&entry.id)?
            .ok_or_else(|| CoreError::data("entry missing after its event"))
    }

    /// Record the first write of an entry, or a later one, choosing the kind.
    pub fn record_new_entry(
        &self,
        key: &DeviceKey,
        clock: &mut Clock,
        entry: &Entry,
    ) -> Result<Entry> {
        self.record_new_entry_with_photo(key, clock, entry, None)
    }

    /// [`Self::record_new_entry`], carrying the photo bytes in the event.
    pub fn record_new_entry_with_photo(
        &self,
        key: &DeviceKey,
        clock: &mut Clock,
        entry: &Entry,
        photo: Option<&[u8]>,
    ) -> Result<Entry> {
        let kind = if self.log().for_entry(&entry.id)?.is_empty() {
            EventKind::Create
        } else {
            EventKind::Edit
        };
        self.record_entry_with_photo(key, clock, entry, kind, photo)
    }

    /// Tombstone an entry.
    ///
    /// A delete is an event like any other, which is what lets it travel: the
    /// row is hidden because the newest change is a delete, on every device.
    pub fn record_delete(&self, key: &DeviceKey, clock: &mut Clock, entry_id: &str) -> Result<()> {
        let entry = self
            .entry(entry_id)?
            .ok_or_else(|| CoreError::NotFound(entry_id.to_string()))?;
        let Some(topic) = entry.baby_id.clone() else {
            return Err(CoreError::invalid("an entry needs a child to belong to"));
        };
        let group = self
            .group_key(&topic)?
            .ok_or_else(|| CoreError::invalid("this device does not hold that child's key"))?;

        let event = Event::seal(
            key,
            &group,
            &topic,
            entry_id,
            EventKind::Delete,
            clock.tick(now_millis()),
            b"",
        )?;
        self.log().insert(&event)?;
        self.project_entry(entry_id, &group)?;
        Ok(())
    }
}

/// Seal one profile write, so creating and renaming cannot drift apart in what
/// they sign.
fn seal_profile(
    key: &DeviceKey,
    clock: &mut Clock,
    group: &GroupKey,
    baby_id: &str,
    baby: &Baby,
    photo: Option<&[u8]>,
) -> Result<Event> {
    let payload = BabyPayload::of_with_photo(baby, photo).to_bytes()?;
    Event::seal(
        key,
        group,
        baby_id,
        baby_id,
        EventKind::Profile,
        clock.tick(now_millis()),
        &payload,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Theme;

    struct Device {
        key: DeviceKey,
        clock: Clock,
        repo: Repo,
    }

    impl Device {
        fn new() -> Self {
            let repo = Repo::open_in_memory().unwrap();
            let key = repo.device_key().unwrap();
            let clock = repo.clock(key.id()).unwrap();
            Self { key, clock, repo }
        }

        fn create(&mut self, name: &str) -> Baby {
            self.repo
                .create_baby(&self.key, &mut self.clock, name, "2026-02-14")
                .unwrap()
        }
    }

    #[test]
    fn creating_a_baby_authors_an_event_and_keeps_its_key() {
        let mut device = Device::new();

        let baby = device.create("Ada");
        let id = baby.id.clone().unwrap();

        assert_eq!(device.repo.babies().unwrap().len(), 1);
        let events = device.repo.log().for_entry(&id).unwrap();
        assert_eq!(events.len(), 1, "one profile event, not a bare row");
        assert_eq!(events[0].kind, EventKind::Profile);
        assert_eq!(events[0].topic, id, "the baby is its own topic");
        assert!(
            device.repo.group_key(&id).unwrap().is_some(),
            "the key has to survive a restart"
        );
    }

    #[test]
    fn renaming_a_baby_appends_a_newer_event() {
        let mut device = Device::new();
        let baby = device.create("Ada");
        let id = baby.id.clone().unwrap();

        let renamed = device
            .repo
            .save_profile(
                &device.key,
                &mut device.clock,
                &Baby {
                    name: "Ada Lovelace".into(),
                    ..baby
                },
            )
            .unwrap();

        assert_eq!(renamed.name, "Ada Lovelace");
        assert_eq!(
            renamed.born, "2026-02-14",
            "the rest of the profile is kept"
        );
        assert_eq!(
            device.repo.log().for_entry(&id).unwrap().len(),
            2,
            "an edit is an event, not an overwrite"
        );
        assert_eq!(device.repo.babies().unwrap().len(), 1);
    }

    #[test]
    fn a_profile_picture_travels_in_the_event_and_survives_a_rename() {
        let mut device = Device::new();
        let baby = device.create("Ada");
        let id = baby.id.clone().unwrap();
        let photo: &[u8] = b"jpeg-ish bytes";
        let pictured = Baby {
            photo: Some(crate::photo::baby_photo_id(&id) + ".jpg"),
            ..baby.clone()
        };

        device
            .repo
            .save_profile_with_photo(&device.key, &mut device.clock, &pictured, Some(photo))
            .unwrap();

        assert_eq!(
            device.repo.baby_by_id(&id).unwrap().unwrap().photo,
            pictured.photo,
            "the row records the file name"
        );
        assert_eq!(
            device.repo.baby_photo_bytes(&id).unwrap().as_deref(),
            Some(photo),
            "a peer can read the bytes out of the event"
        );

        // An edit from a device that has no local file must not drop it.
        let renamed = device
            .repo
            .save_profile(
                &device.key,
                &mut device.clock,
                &Baby {
                    name: "Ada Lovelace".into(),
                    ..pictured
                },
            )
            .unwrap();

        assert_eq!(renamed.name, "Ada Lovelace");
        assert_eq!(
            device.repo.baby_photo_bytes(&id).unwrap().as_deref(),
            Some(photo),
            "the older profile event still carries the picture"
        );
    }

    #[test]
    fn a_restarted_clock_never_sorts_before_the_log() {
        // The failure this prevents: a device restarts, its clock starts at
        // zero, and its next write loses to an event it already has.
        let mut device = Device::new();
        device.create("Ada");
        let newest = device
            .repo
            .log()
            .heads()
            .unwrap()
            .into_iter()
            .max()
            .unwrap();

        let mut restarted = device.repo.clock(device.key.id()).unwrap();
        let next = restarted.tick(now_millis());

        assert!(
            next > newest,
            "a restarted clock must sort after everything already logged"
        );
    }

    #[test]
    fn changing_the_theme_publishes_nothing() {
        let mut device = Device::new();
        device.create("Ada");
        let before = device.repo.log().count().unwrap();

        device.repo.save_device_theme(Theme::Dark).unwrap();

        assert_eq!(device.repo.log().count().unwrap(), before, "local only");
        assert_eq!(device.repo.device_theme().unwrap(), Theme::Dark);
    }

    #[test]
    fn logging_an_entry_writes_an_event_and_folds_the_row() {
        let mut device = Device::new();
        let baby = device.create("Ada");
        let mut entry = Entry::new(crate::model::EntryKind::Feed, 1_700_000_000_000);
        entry.baby_id = baby.id.clone();

        let stored = device
            .repo
            .record_new_entry(&device.key, &mut device.clock, &entry)
            .unwrap();

        assert_eq!(stored.id, entry.id);
        assert_eq!(device.repo.entries().unwrap().len(), 1);
        let events = device.repo.log().for_entry(&entry.id).unwrap();
        assert_eq!(events.len(), 1, "the row is a fold of one event");
        assert_eq!(events[0].kind, EventKind::Create);
        assert_eq!(events[0].topic, baby.id.clone().unwrap());
    }

    #[test]
    fn a_photo_travels_in_the_event_and_survives_an_edit_without_it() {
        let mut device = Device::new();
        let baby = device.create("Ada");
        let mut entry = Entry::new(crate::model::EntryKind::Diaper, 1_700_000_000_000);
        entry.baby_id = baby.id.clone();
        let photo: &[u8] = b"jpeg-ish bytes";

        device
            .repo
            .record_new_entry_with_photo(&device.key, &mut device.clock, &entry, Some(photo))
            .unwrap();
        assert_eq!(
            device.repo.photo_bytes(&entry.id).unwrap().as_deref(),
            Some(photo)
        );

        // An edit made on a device that has no local photo file must not drop
        // the image: the older event still carries it.
        let mut changed = entry.clone();
        changed.note = Some("from the other phone".into());
        device
            .repo
            .record_new_entry_with_photo(&device.key, &mut device.clock, &changed, None)
            .unwrap();
        assert_eq!(
            device.repo.photo_bytes(&entry.id).unwrap().as_deref(),
            Some(photo)
        );
    }

    #[test]
    fn a_second_change_to_an_entry_is_an_edit_not_a_second_entry() {
        let mut device = Device::new();
        let baby = device.create("Ada");
        let mut entry = Entry::new(crate::model::EntryKind::Feed, 1_700_000_000_000);
        entry.baby_id = baby.id.clone();
        device
            .repo
            .record_new_entry(&device.key, &mut device.clock, &entry)
            .unwrap();

        let mut changed = entry.clone();
        changed.note = Some("took it all".into());
        let stored = device
            .repo
            .record_new_entry(&device.key, &mut device.clock, &changed)
            .unwrap();

        assert_eq!(stored.note.as_deref(), Some("took it all"));
        assert_eq!(device.repo.entries().unwrap().len(), 1, "still one entry");
        let events = device.repo.log().for_entry(&entry.id).unwrap();
        assert_eq!(events.len(), 2, "an edit is an event, not an overwrite");
        assert_eq!(events[1].kind, EventKind::Edit);
    }

    #[test]
    fn a_delete_tombstones_the_entry_and_travels_like_any_other_change() {
        let mut device = Device::new();
        let baby = device.create("Ada");
        let mut entry = Entry::new(crate::model::EntryKind::Feed, 1_700_000_000_000);
        entry.baby_id = baby.id.clone();
        device
            .repo
            .record_new_entry(&device.key, &mut device.clock, &entry)
            .unwrap();

        device
            .repo
            .record_delete(&device.key, &mut device.clock, &entry.id)
            .unwrap();

        assert!(device.repo.entries().unwrap().is_empty(), "hidden locally");
        let events = device.repo.log().for_entry(&entry.id).unwrap();
        assert_eq!(events.last().unwrap().kind, EventKind::Delete);
    }

    #[test]
    fn an_entry_for_a_child_whose_key_we_lack_is_refused() {
        let mut device = Device::new();
        let mut entry = Entry::new(crate::model::EntryKind::Feed, 1_700_000_000_000);
        entry.baby_id = Some(crate::model::new_id());

        assert!(device
            .repo
            .record_new_entry(&device.key, &mut device.clock, &entry)
            .is_err());
        assert_eq!(device.repo.entries().unwrap().len(), 0);
    }

    #[test]
    fn saving_a_profile_for_an_unknown_baby_is_an_error() {
        // Without the group key there is nothing to seal with, and inventing one
        // would make a second, unreadable topic for the same baby.
        let mut device = Device::new();
        let stranger = Baby {
            id: Some(new_id()),
            name: "Nobody".into(),
            ..Baby::default()
        };

        assert!(device
            .repo
            .save_profile(&device.key, &mut device.clock, &stranger)
            .is_err());
    }
}
