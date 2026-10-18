import {
  AMOUNTS,
  CONSISTENCIES,
  DIAPER_TYPES,
  FEED_KINDS,
  PEE_SHADES,
  POOP_COLORS,
  SLEEP_LOCATIONS,
  WAKE_MOODS,
  labelOf,
} from "./data/model";
import type { Entry } from "./data/model";

/** Escape text for use in generated HTML. */
export function esc(value: unknown): string {
  return String(value ?? "").replace(/[&<>"]/g, (char) =>
    char === "&" ? "&amp;" : char === "<" ? "&lt;" : char === ">" ? "&gt;" : "&quot;",
  );
}

/** 3:42 PM */
export function hm(time: number): string {
  return new Date(time).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

/** "14:05" for <input type="time"> values. */
export function tv(time: number): string {
  const date = new Date(time);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

/** Today's date at the given HH:MM (ms). */
export function setT(base: number, value: string): number {
  const date = new Date(base);
  const [hours, minutes] = (value || "0:0").split(":");
  date.setHours(Number(hours), Number(minutes), 0, 0);
  return date.getTime();
}

/** 1h 20m */
export function dur(ms: number): string {
  const minutes = Math.max(0, Math.round(ms / 60000));
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

/** just now / 12 min ago / 1h 20m ago */
export function ago(time: number): string {
  const minutes = Math.round((Date.now() - time) / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m ago`;
}

export function isToday(time: number): boolean {
  return new Date(time).toDateString() === new Date().toDateString();
}

/** Local midnight for the day containing `date`. */
export function dayStart(date: Date): number {
  const start = new Date(date);
  start.setHours(0, 0, 0, 0);
  return start.getTime();
}

/** "9 days old" / "3 weeks old" / "4 months old". */
export function ageStr(birthday: string): string {
  const days = Math.floor((Date.now() - new Date(birthday).getTime()) / 86_400_000);
  if (days < 0) return "";
  if (days < 14) return `${days} days old`;
  if (days < 84) return `${Math.floor(days / 7)} weeks old`;
  return `${Math.floor(days / 30.4)} months old`;
}

/** Nothing is ever logged in the future — clamp clock-skewed picks to now. */
export function capNow(time: number): number {
  return time > Date.now() ? Date.now() : time;
}

export function sleepMs(entry: Entry, now: number = Date.now()): number {
  return (entry.ended_at ?? now) - entry.occurred_at;
}

/* --------------------------------------------------------------- labels */

export function entryTitle(entry: Entry): string {
  if (entry.kind === "sleep") return `Slept ${dur(sleepMs(entry))}`;
  if (entry.kind === "feed") return labelOf(FEED_KINDS, entry.feed_kind);
  return labelOf(DIAPER_TYPES, entry.diaper_type);
}

export function entryDetail(entry: Entry): string {
  if (entry.kind === "sleep") {
    const extras = [labelOf(SLEEP_LOCATIONS, entry.sleep_location), labelOf(WAKE_MOODS, entry.wake_mood)]
      .filter(Boolean)
      .join(" · ");
    const times = `${hm(entry.occurred_at)} to ${entry.ended_at ? hm(entry.ended_at) : "now"}`;
    return extras ? `${times} · ${extras}` : times;
  }
  if (entry.kind === "feed") {
    return entry.amount ? `${entry.amount} ${entry.amount_unit}` : "";
  }
  const parts: string[] = [];
  if (entry.diaper_type !== "poop") {
    const pee = [labelOf(PEE_SHADES, entry.pee_shade), labelOf(AMOUNTS, entry.pee_amount)]
      .filter(Boolean)
      .join(" · ");
    if (pee) parts.push(`Pee: ${pee}`);
  }
  if (entry.diaper_type !== "pee") {
    const poop = [
      labelOf(POOP_COLORS, entry.poop_color),
      labelOf(CONSISTENCIES, entry.consistency),
      labelOf(AMOUNTS, entry.poop_amount),
    ]
      .filter(Boolean)
      .join(", ");
    if (poop) parts.push(`Poop: ${poop}`);
  }
  return parts.join(" · ");
}

/** CSV "what" column — mirrors little-log-core's export. */
export function entryWhat(entry: Entry): string {
  if (entry.kind === "sleep") return "Sleep";
  if (entry.kind === "feed") return labelOf(FEED_KINDS, entry.feed_kind);
  return labelOf(DIAPER_TYPES, entry.diaper_type);
}

/** CSV "details" column — duration for sleep, amount/attributes otherwise. */
export function entryCsvDetails(entry: Entry): string {
  if (entry.kind === "sleep") return dur(sleepMs(entry));
  return entryDetail(entry);
}
