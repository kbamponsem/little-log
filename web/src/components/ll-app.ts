import { defineComponent, LLComponent } from "../core/component";
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
import "./sheets/ll-detail-sheet";
import "./sheets/ll-babies-sheet";
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

  protected template(): string {
    return `
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

    this.applyTheme();
    this.syncViews();
  }

  protected override disconnect(): void {
    window.clearInterval(this.joinPoll);
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
}

defineComponent("ll-app", LlApp);
