import { v7 as uuidv7 } from "uuid";

/* ------------------------------------------------------------ value types */

export type EntryKind = "feed" | "sleep" | "diaper";
export type FeedKind = "breast_left" | "breast_right" | "bottle" | "solids";
export type DiaperType = "pee" | "poop" | "both";
export type PeeShade = "pale" | "normal" | "dark";
export type Amount = "small" | "medium" | "large";
export type PoopColor =
  | "mustard"
  | "yellow"
  | "green"
  | "brown"
  | "dark"
  | "pale"
  | "red_streaked";
export type Consistency = "liquid" | "runny" | "soft" | "pasty" | "firm" | "hard";
export type SleepLocation = "crib" | "arms" | "stroller" | "car" | "parents_bed";
export type WakeMood = "calm" | "fussy" | "cried";
export type Role = "owner" | "co_parent" | "caregiver";
export type Theme = "auto" | "light" | "dark";
/** Where a device's request to join a child's log has got to. */
export type JoinState = "pending" | "accepted" | "declined";
export type AmountUnit = "ml" | "min";
export type TabName = "today" | "hist" | "trend" | "baby";

/* ---------------------------------------------------------------- records */

/** One logged event. Mirrors `little-log-core`'s `Entry`. */
export interface Entry {
  id: string;
  baby_id: string | null;
  kind: EntryKind;
  /** Start of the event (for sleep) or the time it happened, ms since epoch UTC. */
  occurred_at: number;
  /** Sleep end; `null` while a sleep is still running. */
  ended_at: number | null;
  feed_kind: FeedKind | null;
  amount: number | null;
  amount_unit: AmountUnit | null;
  diaper_type: DiaperType | null;
  pee_shade: PeeShade | null;
  pee_amount: Amount | null;
  poop_color: PoopColor | null;
  consistency: Consistency | null;
  poop_amount: Amount | null;
  sleep_location: SleepLocation | null;
  wake_mood: WakeMood | null;
  note: string | null;
  /** File name of the photo on disk, e.g. "<entryId>.jpg". */
  photo: string | null;
  caregiver_id: string | null;
  created_at: number;
  updated_at: number;
  /** Soft-delete tombstone: 0 | 1. */
  deleted: 0 | 1;
}

/** What the inviter said about a join request, and something to show for it. */
export interface JoinProgress {
  state: JoinState;
  message: string;
}

/** A device waiting to be let into a child's log. */
export interface JoinRequest {
  topic: string;
  /** The asking device's id — the hex of its public key. */
  device: string;
  role: Role;
  requested_at: number;
  state: JoinState;
}

/** How the last sync attempt with one peer went. */
export type PeerSyncState = "idle" | "syncing" | "ok" | "error";

/** One device this log knows how to reach. */
export interface SyncPeer {
  /** The stored ticket; useful for diagnostics. */
  addr: string;
  /** The peer's device id, when the ticket decodes. */
  peer_id: string | null;
  state: PeerSyncState;
  last_synced_at: number | null;
  /** What this peer moved in the last completed round. */
  received: number;
  sent: number;
  error: string | null;
}

/** What the Settings sync card renders. */
export interface SyncStatus {
  running: boolean;
  last_synced_at: number | null;
  /** Totals for the last completed round. */
  received: number;
  sent: number;
  peers: SyncPeer[];
  error: string | null;
}

export interface Baby {
  id?: string;
  name: string;
  /** Birthday as `YYYY-MM-DD`, or "". */
  born: string;
  /** File name of the profile picture in `photos/`, when set. */
  photo?: string | null;
}

export interface Caregiver {
  id?: string;
  name: string;
  email?: string;
  role: Role;
}

export type NewEntry = Pick<Entry, "kind" | "occurred_at"> & Partial<Entry>;

/* ------------------------------------------------------------ option lists */

export interface Labeled<T extends string> {
  id: T;
  label: string;
}

export interface ChoiceOption<T extends string = string> extends Labeled<T> {
  /** Icon name (see icons.ts). */
  icon?: string;
  /** Several icons rendered together (the diaper "Both" card). */
  icons?: string[];
  /** Colour dot (poop colours). */
  dot?: string;
}

export const FEED_KINDS: ChoiceOption<FeedKind>[] = [
  { id: "breast_left", label: "Breast left", icon: "breast_left" },
  { id: "breast_right", label: "Breast right", icon: "breast_right" },
  { id: "bottle", label: "Bottle", icon: "milk" },
  { id: "solids", label: "Solids", icon: "utensils" },
];

export const DIAPER_TYPES: ChoiceOption<DiaperType>[] = [
  { id: "pee", label: "Pee", icon: "droplet" },
  { id: "poop", label: "Poop", icon: "poop" },
  { id: "both", label: "Both", icons: ["droplet", "poop"] },
];

export const PEE_SHADES: Labeled<PeeShade>[] = [
  { id: "pale", label: "Pale" },
  { id: "normal", label: "Normal" },
  { id: "dark", label: "Dark" },
];

export const AMOUNTS: Labeled<Amount>[] = [
  { id: "small", label: "Small" },
  { id: "medium", label: "Medium" },
  { id: "large", label: "Large" },
];

export const POOP_COLORS: ChoiceOption<PoopColor>[] = [
  { id: "mustard", label: "Mustard", dot: "#d4a82a" },
  { id: "yellow", label: "Yellow", dot: "#e6cf4a" },
  { id: "green", label: "Green", dot: "#5a7a2a" },
  { id: "brown", label: "Brown", dot: "#6b4423" },
  { id: "dark", label: "Dark", dot: "#2a1a10" },
  { id: "pale", label: "Pale", dot: "#d9d2b8" },
  { id: "red_streaked", label: "Red-streaked", dot: "#a8322d" },
];

export const CONSISTENCIES: Labeled<Consistency>[] = [
  { id: "liquid", label: "Liquid" },
  { id: "runny", label: "Runny" },
  { id: "soft", label: "Soft" },
  { id: "pasty", label: "Pasty" },
  { id: "firm", label: "Firm" },
  { id: "hard", label: "Hard" },
];

export const SLEEP_LOCATIONS: Labeled<SleepLocation>[] = [
  { id: "crib", label: "Crib" },
  { id: "arms", label: "Arms" },
  { id: "stroller", label: "Stroller" },
  { id: "car", label: "Car" },
  { id: "parents_bed", label: "Parents’ bed" },
];

export const WAKE_MOODS: Labeled<WakeMood>[] = [
  { id: "calm", label: "Calm" },
  { id: "fussy", label: "Fussy" },
  { id: "cried", label: "Cried" },
];

export const THEMES: Labeled<Theme>[] = [
  { id: "auto", label: "Auto" },
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" },
];

export const INVITE_ROLES: Labeled<Extract<Role, "co_parent" | "caregiver">>[] = [
  { id: "co_parent", label: "Co-parent" },
  { id: "caregiver", label: "Caregiver" },
];

export const ROLE_LABELS: Record<Role, string> = {
  owner: "Owner",
  co_parent: "Co-parent · edit all",
  caregiver: "Caregiver · view and add",
};

export const TABS: Array<{ id: TabName; icon: string; label: string }> = [
  { id: "today", icon: "house", label: "Today" },
  { id: "hist", icon: "book-open", label: "History" },
  { id: "trend", icon: "chart-column", label: "Trends" },
  { id: "baby", icon: "baby", label: "Baby" },
];

/** Poop colours worth mentioning to a pediatrician. */
export const ALERT_POOP_COLORS: PoopColor[] = ["red_streaked", "dark", "pale"];

/* ---------------------------------------------------------------- helpers */

export function labelOf<T extends string>(
  options: readonly Labeled<T>[],
  id: T | null | undefined,
): string {
  return options.find((option) => option.id === id)?.label ?? (id ?? "");
}

/** Build a fully-populated entry with device-generated UUIDv7 id (idempotent writes). */
export function newEntry(fields: NewEntry): Entry {
  const now = Date.now();
  return {
    id: uuidv7(),
    baby_id: null,
    ended_at: null,
    feed_kind: null,
    amount: null,
    amount_unit: null,
    diaper_type: null,
    pee_shade: null,
    pee_amount: null,
    poop_color: null,
    consistency: null,
    poop_amount: null,
    sleep_location: null,
    wake_mood: null,
    note: null,
    photo: null,
    caregiver_id: null,
    created_at: now,
    updated_at: now,
    deleted: 0,
    ...fields,
  };
}
