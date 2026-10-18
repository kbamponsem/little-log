import { dayStart } from "../format";
import type { TabName } from "../data/model";
import { Observable } from "./observable";

/** Every bottom sheet the app can host. */
export type SheetName = "feed" | "diaper" | "sleep" | "detail";

/** History screen filter, including the photo gallery. */
export type HistoryFilter = "all" | "feed" | "sleep" | "diaper" | "photo";

/** Roles a user can be invited as. */
export type InviteRole = "co_parent" | "caregiver";

/** What to open in the sheet host. */
export interface SheetRequest {
  name: SheetName;
  /** Detail sheet only. */
  entryId?: string;
}

/**
 * UI-only state (never persisted): which screen is showing, which sheet is
 * open, the history cursor, and the invite role being picked.
 */
export class UiStore extends Observable<UiStore> {
  tab: TabName = "today";
  sheet: SheetRequest | null = null;
  /** True once boot has loaded the log (see main.ts). */
  booted = false;
  historyDay: number = dayStart(new Date());
  historyFilter: HistoryFilter = "all";
  inviteRole: InviteRole = "co_parent";

  go(tab: TabName): void {
    if (this.tab === tab && !this.sheet) return;
    this.tab = tab;
    this.notify();
  }

  openSheet(request: SheetRequest): void {
    this.sheet = request;
    this.notify();
  }

  closeSheet(): void {
    if (!this.sheet) return;
    this.sheet = null;
    this.notify();
  }

  /** Called once by main.ts after the log has been loaded. */
  markBooted(): void {
    if (this.booted) return;
    this.booted = true;
    this.notify();
  }

  setHistoryDay(time: number): void {
    this.historyDay = time;
    this.notify();
  }

  setHistoryFilter(filter: HistoryFilter): void {
    if (this.historyFilter === filter) return;
    this.historyFilter = filter;
    this.notify();
  }

  setInviteRole(role: InviteRole): void {
    if (this.inviteRole === role) return;
    this.inviteRole = role;
    this.notify();
  }
}

export const ui = new UiStore();

/** Custom event types used to talk between components. */
export const LL = {
  /** An entry row / photo was tapped: `detail = { id }`. */
  openEntry: "ll-open-entry",
  /** A big action tile was tapped: `detail = { kind }`. */
  tilePress: "ll-tile-press",
  /** The history day cursor moved: `detail = { delta }`. */
  dayChange: "ll-day-change",
} as const;

export type OpenEntryDetail = { id: string };
export type TilePressDetail = { kind: "feed" | "sleep" | "diaper" };
export type DayChangeDetail = { delta: number };
