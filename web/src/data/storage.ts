import { invoke } from "@tauri-apps/api/core";
import type {
  Baby,
  Caregiver,
  Entry,
  JoinProgress,
  JoinRequest,
  MoodSchedule,
  Role,
  SyncStatus,
  Theme,
} from "./model";

/**
 * The storage adapter contract.
 *
 * There is one implementation: the app, talking IPC to `little-log-core`
 * against SQLite on the device. The browser adapter is gone — the browser is no
 * longer a target (see PLAN.md) — so everything here runs offline and nothing
 * needs a session.
 */
export interface StorageAdapter {
  /** This device's id: the hex of its public key, generated on first open. */
  identity(): Promise<string>;

  /** This device's appearance preference. Local; never replicated. */
  theme(): Promise<Theme>;
  saveTheme(theme: Theme): Promise<void>;

  /** This device's daily mood check-in reminder. Local; never replicated. */
  moodSchedule(): Promise<MoodSchedule>;
  saveMoodSchedule(schedule: MoodSchedule): Promise<void>;

  listEntries(): Promise<Entry[]>;
  saveEntry(entry: Entry): Promise<Entry>;
  deleteEntry(entryId: string): Promise<void>;

  babies(): Promise<Baby[]>;
  /** The child this device is looking at, or `null` when there is none yet. */
  currentBaby(): Promise<Baby | null>;
  /** Add a child: a fresh id, its own group key and a first profile event. */
  createBaby(name: string, born: string): Promise<Baby>;
  saveBaby(baby: Baby): Promise<Baby>;
  /** Point this device at a child. Local only — it changes nobody else's view. */
  setCurrentBaby(babyId: string): Promise<void>;

  caregivers(): Promise<Caregiver[]>;
  /** A ready-to-send invite link for the current child. */
  invite(role: Role): Promise<string>;
  /**
   * Ask to join from an invite link. Returns as soon as the ask is sent: whether
   * this device is let in is someone else's decision, so the UI polls
   * `pollJoin` and shows "waiting" until it lands.
   */
  requestJoin(link: string): Promise<void>;
  /** Where a join request stands. Polls the inviter. */
  pollJoin(link: string): Promise<JoinProgress>;
  /** Devices waiting to be let into the current child's log. */
  joinRequests(): Promise<JoinRequest[]>;
  /** Let one in, or turn it down. Owner or co-parent only. */
  resolveJoin(device: string, accept: boolean): Promise<void>;

  /** Current sync state and per-peer progress. */
  syncStatus(): Promise<SyncStatus>;
  /** Start a sync round now; returns the state immediately after it starts. */
  forceSync(): Promise<SyncStatus>;

  csv(): Promise<string>;
  deleteAll(): Promise<void>;

  /**
   * The action a widget tap asked the app to open, if one is waiting. Read
   * once — the native side clears it — so a sheet opens on the tap that asked
   * for it and not on every resume.
   */
  takeWidgetAction(): Promise<string | null>;

  /** Stores the photo and returns the entry's photo file name. */
  savePhoto(entryId: string, blob: Blob): Promise<string>;
  /** Points every <img data-eid> under `root` at its photo. */
  fillPhotos(root: HTMLElement): void | Promise<void>;
  /** Stores a child's profile picture and returns its file name. */
  saveBabyPhoto(babyId: string, blob: Blob): Promise<string>;
  /** A data URL for a child's profile picture. */
  babyPhotoDataUrl(babyId: string): Promise<string>;
}

/**
 * App adapter: IPC into `little-log-core` against SQLite on the device.
 *
 * There is no login and no session: the device's key *is* its identity, so
 * `identity()` is the only auth-shaped call left, and it always succeeds.
 */
export class TauriAdapter implements StorageAdapter {
  private call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
    return invoke<T>(command, args);
  }

  async identity(): Promise<string> {
    const identity = await this.call<{ id: string }>("device_identity");
    return identity.id;
  }

  /* device settings */

  theme(): Promise<Theme> {
    return this.call<Theme>("device_theme");
  }

  saveTheme(theme: Theme): Promise<void> {
    return this.call<void>("save_device_theme", { theme });
  }

  moodSchedule(): Promise<MoodSchedule> {
    return this.call<MoodSchedule>("device_mood_schedule");
  }

  saveMoodSchedule(schedule: MoodSchedule): Promise<void> {
    return this.call<void>("save_device_mood_schedule", { schedule });
  }

  /* entries */

  listEntries(): Promise<Entry[]> {
    return this.call<Entry[]>("list_entries");
  }

  saveEntry(entry: Entry): Promise<Entry> {
    return this.call<Entry>("save_entry", { entry });
  }

  deleteEntry(entryId: string): Promise<void> {
    return this.call<void>("delete_entry", { entryId });
  }

  /* children + caregivers */

  babies(): Promise<Baby[]> {
    return this.call<Baby[]>("list_babies");
  }

  currentBaby(): Promise<Baby | null> {
    return this.call<Baby | null>("current_baby");
  }

  createBaby(name: string, born: string): Promise<Baby> {
    return this.call<Baby>("create_baby", { name, born });
  }

  saveBaby(baby: Baby): Promise<Baby> {
    return this.call<Baby>("save_baby", { baby });
  }

  setCurrentBaby(babyId: string): Promise<void> {
    return this.call<void>("set_current_baby", { babyId });
  }

  caregivers(): Promise<Caregiver[]> {
    return this.call<Caregiver[]>("list_caregivers");
  }

  invite(role: Role): Promise<string> {
    return this.call<string>("create_invite", { role });
  }

  requestJoin(link: string): Promise<void> {
    return this.call<void>("request_join", { link });
  }

  pollJoin(link: string): Promise<JoinProgress> {
    return this.call<JoinProgress>("poll_join", { link });
  }

  joinRequests(): Promise<JoinRequest[]> {
    return this.call<JoinRequest[]>("join_requests");
  }

  resolveJoin(device: string, accept: boolean): Promise<void> {
    return this.call<void>("resolve_join", { device, accept });
  }

  syncStatus(): Promise<SyncStatus> {
    return this.call<SyncStatus>("sync_status");
  }

  forceSync(): Promise<SyncStatus> {
    return this.call<SyncStatus>("sync_now");
  }

  /* data */

  csv(): Promise<string> {
    return this.call<string>("export_csv");
  }

  deleteAll(): Promise<void> {
    return this.call<void>("delete_all");
  }

  takeWidgetAction(): Promise<string | null> {
    return this.call<string | null>("take_widget_action");
  }

  async savePhoto(entryId: string, blob: Blob): Promise<string> {
    const bytes = new Uint8Array(await blob.arrayBuffer());
    return this.call<string>("save_photo", { entryId, bytes: Array.from(bytes) });
  }

  async fillPhotos(root: HTMLElement): Promise<void> {
    for (const img of root.querySelectorAll<HTMLImageElement>("img[data-eid]:not([src])")) {
      try {
        img.src = await this.call<string>("photo_data_url", { entryId: img.dataset.eid ?? "" });
      } catch {
        /* photo missing on disk — leave the placeholder */
      }
    }
  }

  async saveBabyPhoto(babyId: string, blob: Blob): Promise<string> {
    const bytes = new Uint8Array(await blob.arrayBuffer());
    return this.call<string>("save_baby_photo", { babyId, bytes: Array.from(bytes) });
  }

  babyPhotoDataUrl(babyId: string): Promise<string> {
    return this.call<string>("baby_photo_data_url", { babyId });
  }
}

/** The adapter for this runtime. */
export const storage: StorageAdapter = new TauriAdapter();
