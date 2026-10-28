import { dayStart } from "../format";
import type { JoinRequest, TabName } from "../data/model";
import { Observable } from "./observable";

/** Every bottom sheet the app can host. */
export type SheetName =
  | "feed"
  | "diaper"
  | "sleep"
  | "mood"
  | "detail"
  | "babies"
  | "add-baby"
  | "theme"
  | "invite"
  | "join";

/** History screen filter, including the photo gallery. */
export type HistoryFilter = "all" | "feed" | "sleep" | "diaper" | "mood" | "photo";

/** Roles a user can be invited as. */
export type InviteRole = "co_parent" | "caregiver";

/** A full-screen view that is not one of the four tabs. */
export type OverlayName = "settings";

/** A transient message: "Joined Ada", "They declined", and so on. */
export type ToastKind = "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

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
  /** A full-screen view over the tabs, or null for the normal tab bar. */
  overlay: OverlayName | null = null;
  sheet: SheetRequest | null = null;
  /** Live transient messages; `ll-toast-host` renders and retires them. */
  toasts: Toast[] = [];
  private nextToastId = 1;
  /** A device waiting to be let in, presented as a blocking prompt. */
  joinPrompt: JoinRequest | null = null;
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

  /** Show a full-screen view over the tabs (e.g. Settings from the cog). */
  openOverlay(name: OverlayName): void {
    if (this.overlay === name) return;
    this.overlay = name;
    this.notify();
  }

  closeOverlay(): void {
    if (!this.overlay) return;
    this.overlay = null;
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

  /** Present one waiting device, or clear the prompt once it is dealt with. */
  setJoinPrompt(request: JoinRequest | null): void {
    if (this.joinPrompt === request) return;
    this.joinPrompt = request;
    this.notify();
  }

  /* -------------------------------------------------------------- toasts */

  /**
   * Show a transient message.
   *
   * Fired from wherever the news comes from — a sheet, a screen, the widget
   * handoff — and timed out by `ll-toast-host`, so whoever reported it does not
   * have to still be on screen when it lands.
   */
  toast(kind: ToastKind, message: string): void {
    this.toasts = [...this.toasts, { id: this.nextToastId++, kind, message }];
    this.notify();
  }

  dismissToast(id: number): void {
    const next = this.toasts.filter((toast) => toast.id !== id);
    if (next.length === this.toasts.length) return;
    this.toasts = next;
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
export type TilePressDetail = { kind: "feed" | "sleep" | "diaper" | "mood" };
export type DayChangeDetail = { delta: number };
