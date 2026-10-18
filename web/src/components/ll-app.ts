import { defineComponent, LLComponent } from "../core/component";
import { LL, ui } from "../core/ui-store";
import type { OpenEntryDetail } from "../core/ui-store";
import { store } from "../data/store";
import type { TabName } from "../data/model";
import "./ll-app.css";
import "./ui/ll-sheet-host";
import "./ui/ll-tabs";
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
 * Composition root. Owns the four screens, the tab bar and the sheet host,
 * routes the events the components emit, and keeps the theme in sync with the
 * baby's preference. Boot lives in main.ts.
 *
 * There is no sign-in screen: a device's key *is* its identity (see
 * `little_log_core::keystore`), so the app opens straight onto Today.
 */
export class LlApp extends LLComponent {
  protected template(): string {
    return `
      <div class="splash" data-role="splash"><span>Little Log</span></div>
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
  }

  protected override subscribe(): void {
    this.watch(store, () => this.applyTheme());
    this.watch(ui, () => this.syncViews());
    this.applyTheme();
    this.syncViews();
  }

  /** Show the splash until boot finishes, then exactly one screen. */
  private syncViews(): void {
    const splash = this.querySelector<HTMLElement>('[data-role="splash"]');
    if (splash) splash.hidden = ui.booted;

    const tabs = this.querySelector<HTMLElement>("ll-tabs");
    if (tabs) tabs.hidden = !ui.booted;

    for (const [name, tag] of Object.entries(SCREEN_TAGS)) {
      const screen = this.querySelector<HTMLElement>(tag);
      if (screen) screen.hidden = !ui.booted || name !== ui.tab;
    }
  }

  private applyTheme(): void {
    const theme = store.baby.theme;
    if (theme === "auto") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
  }
}

defineComponent("ll-app", LlApp);
