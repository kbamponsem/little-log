import { defineComponent, LLComponent } from "../core/component";
import { exit, onBackButtonPress } from "@tauri-apps/api/app";
import { LL, ui } from "../core/ui-store";
import type { OpenEntryDetail } from "../core/ui-store";
import { store } from "../data/store";
import type { TabName } from "../data/model";
import "./ll-app.css";
import "./ui/ll-sheet-host";
import "./ui/ll-join-prompt";
import "./ui/ll-tabs";
import "./ui/ll-toast-host";
import "./screens/ll-today";
import "./screens/ll-history";
import "./screens/ll-trends";
import "./screens/ll-baby";
import "./screens/ll-welcome";
import "./screens/ll-settings";
import "./sheets/ll-feed-sheet";
import "./sheets/ll-diaper-sheet";
import "./sheets/ll-sleep-sheet";
import "./sheets/ll-mood-sheet";
import "./sheets/ll-detail-sheet";
import "./sheets/ll-babies-sheet";
import "./sheets/ll-add-baby-sheet";
import "./sheets/ll-invite-sheet";
import "./sheets/ll-join-sheet";
import "./sheets/ll-theme-sheet";

const SCREEN_TAGS: Record<TabName, string> = {
  today: "ll-today",
  hist: "ll-history",
  trend: "ll-trends",
  baby: "ll-baby",
};

/**
 * Composition root. Owns the four screens, the tab bar and the sheet host,
 * routes the events the components emit, and keeps the theme in sync with the
 * device's preference. Boot lives in main.ts.
 *
 * There is no sign-in screen: a device's key *is* its identity (see
 * `little_log_core::keystore`). There is an intro screen, but only when the log
 * holds no children at all — a device that syncs into an existing log goes
 * straight to Today.
 */
export class LlApp extends LLComponent {
  /** Whether the intro has been decided, and whether it is still showing. */
  private welcomeDecided = false;
  private showWelcome = false;
  /** Poll for devices waiting to join (see `checkJoinRequests`). */
  private joinPoll = 0;
  /** Unregister the Android back-button listener, when one was set. */
  private backUnregister: (() => void) | null = null;
  /** Pull-to-refresh: the indicator and the gesture's state. */
  private ptrEl: HTMLElement | null = null;
  private ptrStartY = 0;
  private ptrPulling = false;
  private ptrDist = 0;
  private ptrRefreshing = false;

  /** How far a pull must go before it refreshes, and where it holds while it does. */
  private static readonly PTR_THRESHOLD = 64;
  private static readonly PTR_HOLD = 56;

  protected template(): string {
    return `
      <div class="ptr" data-role="ptr"><span class="ptr-spinner"></span></div>
      <div class="splash" data-role="splash"><span>Little Log</span></div>
      <ll-welcome hidden></ll-welcome>
      <ll-today hidden></ll-today>
      <ll-history hidden></ll-history>
      <ll-trends hidden></ll-trends>
      <ll-baby hidden></ll-baby>
      <ll-settings hidden></ll-settings>
      <ll-tabs></ll-tabs>
      <ll-sheet-host></ll-sheet-host>
      <ll-join-prompt></ll-join-prompt>
      <ll-toast-host></ll-toast-host>
    `;
  }

  protected mount(): void {
    this.ptrEl = this.querySelector<HTMLElement>('[data-role="ptr"]');
    // Tile presses are handled by the Today screen, which owns them (ll-today).
    this.on(this, LL.openEntry, (event) => {
      const { id } = (event as CustomEvent<OpenEntryDetail>).detail;
      ui.openSheet({ name: "detail", entryId: id });
    });
    this.on(this, "ll-welcome-done", () => {
      this.showWelcome = false;
      this.syncViews();
    });
  }

  protected override subscribe(): void {
    this.watch(store, () => {
      this.applyTheme();
      this.syncViews();
    });
    this.watch(ui, () => this.syncViews());

    // Join requests are written to SQLite by the sync listener, not through the
    // store, so nothing here would notice one arriving. Poll while the app is in
    // the foreground so the prompt can appear from any tab.
    this.joinPoll = window.setInterval(() => void this.checkJoinRequests(), 4000);

    document.addEventListener("touchstart", this.ptrOnStart, { passive: true });
    document.addEventListener("touchmove", this.ptrOnMove, { passive: false });
    document.addEventListener("touchend", this.ptrOnEnd);
    document.addEventListener("touchcancel", this.ptrOnEnd);

    this.registerBackButton();

    this.applyTheme();
    this.syncViews();
  }

  protected override disconnect(): void {
    window.clearInterval(this.joinPoll);
    document.removeEventListener("touchstart", this.ptrOnStart);
    document.removeEventListener("touchmove", this.ptrOnMove);
    document.removeEventListener("touchend", this.ptrOnEnd);
    document.removeEventListener("touchcancel", this.ptrOnEnd);
    this.backUnregister?.();
    this.backUnregister = null;
    super.disconnect();
  }

  /**
   * Show the splash until boot finishes, then exactly one view: the intro while
   * the log has no children, Settings while the cog is open, otherwise the
   * active tab.
   */
  private syncViews(): void {
    // Decide once, at boot: an existing log goes straight to Today, and a fresh
    // one asks for a child. Adding a child does not dismiss the intro, so
    // "or babies" works — the Start button does.
    if (ui.booted && !this.welcomeDecided) {
      this.welcomeDecided = true;
      this.showWelcome = store.needsIntro;
    }

    const splash = this.querySelector<HTMLElement>('[data-role="splash"]');
    if (splash) splash.hidden = ui.booted;

    const settings = this.querySelector<HTMLElement>("ll-settings");
    const welcome = this.querySelector<HTMLElement>("ll-welcome");
    const onSettings = ui.overlay === "settings";

    if (settings) settings.hidden = !ui.booted || !onSettings;
    // Deliberately not keyed off `store.needsIntro`: adding a child flips that
    // to false straight away, which would hide the intro before its "Added"
    // list and Start button could show. The intro is dismissed by Start, or by
    // a successful join finishing it with `ll-welcome-done`.
    if (welcome) {
      welcome.hidden = !ui.booted || onSettings || !this.showWelcome;
    }

    // The tab bar belongs to the tabs, so it goes away for both overlays.
    const tabs = this.querySelector<HTMLElement>("ll-tabs");
    if (tabs) tabs.hidden = !ui.booted || onSettings || this.showWelcome;

    for (const [name, tag] of Object.entries(SCREEN_TAGS)) {
      const screen = this.querySelector<HTMLElement>(tag);
      if (screen) {
        screen.hidden = !ui.booted || onSettings || this.showWelcome || name !== ui.tab;
      }
    }
  }

  /**
   * Present the first pending request, if there is one and nothing is in the
   * way. Deferring while a sheet is open keeps a feed being logged from being
   * yanked away; the next tick picks the request up.
   */
  private async checkJoinRequests(): Promise<void> {
    if (!ui.booted || document.hidden || ui.joinPrompt || ui.sheet) return;
    if (!store.babies.length) return;
    try {
      const requests = await store.repo.joinRequests();
      const pending = requests.find((request) => request.state === "pending");
      if (pending) ui.setJoinPrompt(pending);
    } catch {
      // No child yet, or sync is not running: there is nothing to prompt for.
    }
  }

  private applyTheme(): void {
    const theme = store.theme;
    if (theme === "auto") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
  }

  /* ------------------------------------------------- Android back button */

  /**
   * Take over the Android back button so it walks the app's own stack instead
   * of closing the activity: sheet → Settings → back to Today → exit.
   */
  private registerBackButton(): void {
    if (!("__TAURI_INTERNALS__" in window)) return;
    void onBackButtonPress(() => this.onBack())
      .then((listener) => {
        this.backUnregister = () => listener.unregister();
      })
      .catch(() => undefined);
  }

  private onBack(): void {
    if (ui.sheet) {
      ui.closeSheet();
      return;
    }
    if (ui.overlay === "settings") {
      ui.closeOverlay();
      return;
    }
    if (ui.tab !== "today") {
      ui.go("today");
      return;
    }
    void exit(0);
  }

  /* ---------------------------------------------------- pull to refresh */

  /**
   * The screen that is on show. Since the shell became one viewport-sized box,
   * the document no longer scrolls: this element does, so pull-to-refresh has
   * to ask it rather than the window.
   */
  private scroller(): HTMLElement | null {
    return this.querySelector<HTMLElement>(
      "ll-today:not([hidden]), ll-history:not([hidden]), ll-trends:not([hidden]), ll-baby:not([hidden]), ll-settings:not([hidden]), ll-welcome:not([hidden])",
    );
  }

  private scrolled(): boolean {
    return (this.scroller()?.scrollTop ?? 0) > 0;
  }

  /** A pull is only meaningful at the top, with nothing open and nothing typed. */
  private canPull(event: TouchEvent): boolean {
    if (this.ptrRefreshing || ui.sheet) return false;
    if (this.scrolled()) return false;
    const active = document.activeElement;
    if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) return false;
    const target = event.target as HTMLElement;
    return !target.closest("ll-sheet-host, .sheet, dialog");
  }

  private readonly ptrOnStart = (event: TouchEvent): void => {
    if (this.ptrRefreshing) return;
    this.ptrStartY = event.touches[0]?.clientY ?? 0;
    this.ptrDist = 0;
    this.ptrPulling = this.canPull(event);
  };

  private readonly ptrOnMove = (event: TouchEvent): void => {
    if (!this.ptrPulling) return;
    const dy = (event.touches[0]?.clientY ?? 0) - this.ptrStartY;
    if (dy <= 0) {
      this.ptrDist = 0;
      this.setPtr(0);
      return;
    }
    if (this.scrolled()) {
      this.ptrPulling = false;
      this.setPtr(0);
      return;
    }
    event.preventDefault();
    this.ptrDist = Math.min(dy * 0.45, 120);
    this.setPtr(this.ptrDist);
  };

  private readonly ptrOnEnd = (): void => {
    if (!this.ptrPulling) return;
    this.ptrPulling = false;
    if (this.ptrDist >= LlApp.PTR_THRESHOLD) void this.runRefresh();
    else this.setPtr(0);
  };

  /** Hold the spinner, reload the log and nudge a sync, then settle. */
  private async runRefresh(): Promise<void> {
    this.ptrRefreshing = true;
    this.setPtr(LlApp.PTR_HOLD, true);
    this.ptrEl?.classList.add("spinning");

    const started = Date.now();
    try {
      await Promise.allSettled([store.refresh(), store.repo.forceSync()]);
    } finally {
      const remaining = 500 - (Date.now() - started);
      if (remaining > 0) await new Promise((resolve) => setTimeout(resolve, remaining));
      this.ptrEl?.classList.remove("spinning");
      this.setPtr(0, true);
      this.ptrRefreshing = false;
      this.ptrDist = 0;
    }
  }

  /** Slide the indicator to `dist` (px from the top); `-48` hides it. */
  private setPtr(dist: number, animate = false): void {
    if (!this.ptrEl) return;
    const y = Math.max(dist - 48, -56);
    this.ptrEl.classList.toggle("ptr-anim", animate);
    this.ptrEl.style.transform = `translate(-50%, ${y}px)`;
  }
}

defineComponent("ll-app", LlApp);
