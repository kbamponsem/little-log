import { Observable } from "../core/observable";
import { isToday } from "../format";
import type { Baby, Entry, EntryKind, MoodSchedule, NewEntry, Theme } from "./model";
import { DEFAULT_MOOD_SCHEDULE, newEntry } from "./model";
import { Repository } from "./repository";
import { storage } from "./storage";

export interface TodayStats {
  feeds: number;
  diapers: number;
  sleepMs: number;
}

/**
 * Observable app state. Components `watch(store)` and re-render on change;
 * every mutation goes through the repository so the adapter stays source of truth.
 */
export class AppStore extends Observable<AppStore> {
  entries: Entry[] = [];
  /** Every child in the log, oldest first. Empty until the intro adds one. */
  babies: Baby[] = [];
  /** Which child this device is looking at. Local, like the theme. */
  currentBabyId: string | null = null;
  /** This device's id — its public key, and the only identity there is. */
  deviceId: string | null = null;
  /** Appearance is a device preference, so it lives here and not on a baby. */
  theme: Theme = "dark";
  /** When this device should prompt for a daily mood check-in. */
  moodSchedule: MoodSchedule = { ...DEFAULT_MOOD_SCHEDULE };

  constructor(readonly repo: Repository) {
    super();
  }

  /**
   * The child the screens show: the one this device points at, falling back to
   * the first — the same rule the repository applies, so a dangling pointer or
   * a fresh install still lands somewhere sensible.
   */
  get baby(): Baby {
    const pointed = this.babies.find((baby) => baby.id === this.currentBabyId);
    return pointed ?? this.babies[0] ?? { name: "Baby", born: "" };
  }

  /**
   * True when the log holds no children at all, which is exactly when the app
   * should ask for one instead of opening onto an empty log. A second device
   * that syncs into an existing log has children, so it is never asked.
   */
  get needsIntro(): boolean {
    return this.babies.length === 0;
  }

  /* -------------------------------------------------------------- loading */

  /** Boot: who this device is, how it looks, the children, then the log. */
  async load(): Promise<void> {
    this.deviceId = await this.repo.identity();
    this.theme = await this.repo.theme();
    this.moodSchedule = await this.repo.moodSchedule();
    this.babies = await this.repo.babies();
    const current = await this.repo.currentBaby();
    this.currentBabyId = current?.id ?? this.babies[0]?.id ?? null;
    await this.refresh();
  }

  /**
   * Reload what the screens show from the database.
   *
   * Children as well as entries: a sync can bring a child this device has never
   * seen — the first thing a joiner receives — and a row list that ignored it
   * would leave the welcome screen asking for a baby that just arrived.
   */
  async refresh(): Promise<void> {
    try {
      this.babies = await this.repo.babies();
      const current = await this.repo.currentBaby();
      this.currentBabyId = current?.id ?? this.babies[0]?.id ?? null;
      this.setEntries(await this.repo.listEntries());
    } catch (error) {
      console.warn("Could not reload the log", error);
    }
  }

  setEntries(entries: Entry[]): void {
    this.entries = [...entries].sort((a, b) => b.occurred_at - a.occurred_at);
    this.notify();
  }

  /* ------------------------------------------------------------ mutations */

  async saveEntry(entry: Entry): Promise<Entry> {
    const saved = await this.repo.saveEntry(entry);
    this.upsert(saved ?? entry);
    return saved ?? entry;
  }

  /** Insert or replace one entry in the local list (newest first). */
  upsert(entry: Entry): void {
    const next = this.entries.filter((candidate) => candidate.id !== entry.id);
    next.push(entry);
    next.sort((a, b) => b.occurred_at - a.occurred_at);
    this.entries = next;
    this.notify();
  }

  /** Log a new entry from a sheet: fills defaults, saves, updates the view. */
  async log(fields: NewEntry): Promise<Entry> {
    return this.saveEntry(newEntry(fields));
  }

  async deleteEntry(entryId: string): Promise<void> {
    await this.repo.deleteEntry(entryId);
    this.entries = this.entries.filter((entry) => entry.id !== entryId);
    this.notify();
  }

  /**
   * Add a child and start looking at it: you have just been typing its name.
   * The child has no entries yet, so the log reloads empty.
   */
  async addBaby(name: string, born: string): Promise<Baby> {
    const baby = await this.repo.createBaby(name, born);
    this.babies = [...this.babies, baby];
    this.currentBabyId = baby.id ?? null;
    this.notify();
    await this.refresh();
    return baby;
  }

  /** Look at a different child. Only this device's pointer moves. */
  async selectBaby(babyId: string): Promise<void> {
    if (this.currentBabyId === babyId) return;
    this.currentBabyId = babyId;
    this.notify();
    await this.repo.setCurrentBaby(babyId);
    await this.refresh();
  }

  async saveBaby(patch: Partial<Baby>): Promise<void> {
    const baby = { ...this.baby, ...patch };
    this.babies = this.babies.map((candidate) => (candidate.id === baby.id ? baby : candidate));
    this.notify();
    const saved = await this.repo.saveBaby(baby);
    this.babies = this.babies.map((candidate) => (candidate.id === saved.id ? saved : candidate));
    this.notify();
  }

  /** Set the current child's profile picture, on disk and in the log. */
  async saveBabyPhoto(blob: Blob): Promise<void> {
    const id = this.baby.id;
    if (!id) return;
    const photo = await this.repo.saveBabyPhoto(id, blob);
    await this.saveBaby({ photo });
  }

  /** Set this device's appearance. Local only — nothing is published. */
  async saveTheme(theme: Theme): Promise<void> {
    this.theme = theme;
    this.notify();
    await this.repo.saveTheme(theme);
  }

  /** Set this device's daily check-in reminder. Local only. */
  async saveMoodSchedule(schedule: MoodSchedule): Promise<void> {
    this.moodSchedule = schedule;
    this.notify();
    await this.repo.saveMoodSchedule(schedule);
  }

  async deleteAll(): Promise<void> {
    await this.repo.deleteAll();
    this.entries = [];
    this.notify();
  }

  /* ------------------------------------------------------------ selectors */

  find(id: string): Entry | undefined {
    return this.entries.find((entry) => entry.id === id);
  }

  /** The sleep that is still running, if the baby is asleep. */
  runningSleep(): Entry | undefined {
    return this.entries.find((entry) => entry.kind === "sleep" && !entry.ended_at);
  }

  lastOf(kind: EntryKind): Entry | undefined {
    return this.entries.find((entry) => entry.kind === kind);
  }

  today(now: number = Date.now()): Entry[] {
    void now;
    return this.entries.filter((entry) => isToday(entry.occurred_at));
  }

  stats(now: number = Date.now()): TodayStats {
    const today = this.today();
    return {
      feeds: today.filter((entry) => entry.kind === "feed").length,
      diapers: today.filter((entry) => entry.kind === "diaper").length,
      sleepMs: today
        .filter((entry) => entry.kind === "sleep")
        .reduce((total, entry) => total + ((entry.ended_at ?? now) - entry.occurred_at), 0),
    };
  }

  /**
   * Whether the daily mood check-in should be showing now.
   *
   * Only after the scheduled local time, and only until a mood has been logged
   * for today — any caregiver's entry counts, because the question is about the
   * baby rather than about this phone.
   */
  moodDue(now: number = Date.now()): boolean {
    if (!this.moodSchedule.enabled) return false;
    const due = new Date(now);
    due.setHours(Math.floor(this.moodSchedule.minute / 60), this.moodSchedule.minute % 60, 0, 0);
    if (now < due.getTime()) return false;
    return !this.today().some((entry) => entry.kind === "mood");
  }
}

/* ------------------------------------------------------------- singletons */

export const repo = new Repository(storage);
export const store = new AppStore(repo);
