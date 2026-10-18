import { dayStart } from "../format";
import type { AuthMode, TabName } from "../data/model";
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

/** What the sign-in screen should show while there is no session. */
export interface AuthRequest {
  mode: AuthMode;
  /** Invite token from the URL, if the app was opened through one. */
  invite: string | null;
}

/**
 * UI-only state (never persisted): which screen is showing, which sheet is
 * open, whether the sign-in screen has taken over, the history cursor, and the
 * invite role being picked.
 */
export class UiStore extends Observable<UiStore> {
  tab: TabName = "today";
  sheet: SheetRequest | null = null;
  /** Non-null while the sign-in screen is the front door. */
  auth: AuthRequest | null = null;
  /** True once the first session check has finished (see main.ts). */
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

  /** Send the user to the sign-in screen (boot, 401, or sign-out). */
  showAuth(request: AuthRequest): void {
    this.auth = request;
    this.notify();
  }

  /** Called once the session is adopted: hand the app back to the screens. */
  hideAuth(): void {
    if (!this.auth) return;
    this.auth = null;
    this.notify();
  }

  /** Called once by main.ts after the boot-time session check. */
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
  /** The auth sheet succeeded: `detail = { session }`. */
  signedIn: "ll-signed-in",
} as const;

export type OpenEntryDetail = { id: string };
export type TilePressDetail = { kind: "feed" | "sleep" | "diaper" };
export type DayChangeDetail = { delta: number };
