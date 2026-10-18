import { defineComponent, LLComponent } from "../core/component";
import { focusInside } from "../core/dom";
import { LL, ui } from "../core/ui-store";
import type { AuthRequest, OpenEntryDetail } from "../core/ui-store";
import { store } from "../data/store";
import type { Session, TabName } from "../data/model";
import type { LlAuth } from "./screens/ll-auth";
import "./ll-app.css";
import "./ui/ll-sheet-host";
import "./ui/ll-tabs";
import "./screens/ll-auth";
import "./screens/ll-today";
import "./screens/ll-history";
import "./screens/ll-trends";
import "./screens/ll-baby";
import "./sheets/ll-feed-sheet";
import "./sheets/ll-diaper-sheet";
import "./sheets/ll-sleep-sheet";
import "./sheets/ll-detail-sheet";

const SCREEN_TAGS: Record<TabName, string> = {
  today: "ll-today",
  hist: "ll-history",
  trend: "ll-trends",
  baby: "ll-baby",
};

/**
 * Composition root. Owns the two top-level views — the sign-in screen and the
 * four-tab app — plus the tab bar and the sheet host, routes the events the
 * components emit, and keeps the theme in sync with the baby's preference.
 * Boot and the 401 hook live in main.ts.
 */
export class LlApp extends LLComponent {
  /** The last `ui.auth` request the sign-in screen was configured with. */
  private lastAuth: AuthRequest | null = null;

  protected template(): string {
    return `
      <div class="splash" data-role="splash"><span>Little Log</span></div>
      <ll-auth hidden></ll-auth>
      <ll-today></ll-today>
      <ll-history hidden></ll-history>
      <ll-trends hidden></ll-trends>
      <ll-baby hidden></ll-baby>
      <ll-tabs></ll-tabs>
      <ll-sheet-host></ll-sheet-host>
    `;
  }

  protected mount(): void {
    // Tile presses are handled by the Today screen, which owns them (ll-today).
    this.on(this, LL.openEntry, (event) => {
      const { id } = (event as CustomEvent<OpenEntryDetail>).detail;
      ui.openSheet({ name: "detail", entryId: id });
    });
    this.on(this, LL.signedIn, (event) => {
      void this.signedIn((event as CustomEvent<{ session: Session }>).detail.session);
    });
  }

  protected override subscribe(): void {
    this.watch(store, () => this.applyTheme());
    this.watch(ui, () => this.syncViews());
    this.applyTheme();
    this.syncViews();
  }

  /* ------------------------------------------------------------- actions */

  private async signedIn(session: Session): Promise<void> {
    const invited = !!ui.auth?.invite;
    store.adoptSession(session);
    if (invited) {
      // The invite is spent; keep it out of the address bar.
      try {
        history.replaceState({}, "", location.pathname);
      } catch {
        /* history unavailable — harmless */
      }
    }
    ui.hideAuth();
    ui.go("today");
    await store.refresh();
  }

  /**
   * Show exactly one top-level view: the splash until boot finishes, then
   * either the sign-in screen or the active tab.
   */
  private syncViews(): void {
    const request = ui.auth;
    const authing = ui.booted && !!request;

    const auth = this.querySelector<LlAuth>("ll-auth");
    if (auth) {
      // Re-configure only when the request changes, so typing survives a notify.
      if (request !== this.lastAuth && request) auth.configure(request);
      this.lastAuth = request;
      const wasHidden = auth.hidden;
      auth.hidden = !authing;
      if (authing && wasHidden) focusInside(auth, "input");
    }

    const splash = this.querySelector<HTMLElement>('[data-role="splash"]');
    if (splash) splash.hidden = ui.booted;

    const tabs = this.querySelector<HTMLElement>("ll-tabs");
    if (tabs) tabs.hidden = authing || !ui.booted;

    for (const [name, tag] of Object.entries(SCREEN_TAGS)) {
      const screen = this.querySelector<HTMLElement>(tag);
      if (screen) screen.hidden = authing || !ui.booted || name !== ui.tab;
    }
  }

  private applyTheme(): void {
    const theme = store.baby.theme;
    if (theme === "auto") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
  }
}

defineComponent("ll-app", LlApp);
