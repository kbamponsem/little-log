import { defineComponent, LLComponent } from "../../core/component";
import { TABS } from "../../data/model";
import type { TabName } from "../../data/model";
import { ic } from "../../icons";
import { ui } from "../../core/ui-store";
import "./ll-tabs.css";

/** How long the Baby tab is held before the child switcher opens. */
const HOLD_MS = 450;

/**
 * The fixed bottom tab bar. Switches `ui.tab`.
 *
 * Holding the Baby tab opens the child switcher, so the home screen does not
 * have to list every child: a one-child household never sees a switcher, and a
 * household with several keeps its header. Tapping the tab still goes to it.
 */
export class LlTabs extends LLComponent {
  private lastTab: TabName | null = null;
  private holdTimer = 0;
  /** True between a hold firing and its release, which must not also navigate. */
  private held = false;

  protected template(): string {
    return TABS.map(
      (tab) =>
        `<button class="tab" type="button" data-tab="${tab.id}"><span class="tab-body"><span class="tab-icon">${ic(tab.icon, 24)}</span><span class="tab-label">${tab.label}</span></span></button>`,
    ).join("");
  }

  protected mount(): void {
    this.addEventListener("click", (event) => {
      const button = (event.target as HTMLElement).closest<HTMLElement>("[data-tab]");
      if (!button) return;
      // The release after a hold: the switcher is already open, so don't switch.
      if (this.held) {
        this.held = false;
        return;
      }
      ui.go(button.dataset.tab as TabName);
    });

    this.addEventListener("pointerdown", (event) => {
      const button = (event.target as HTMLElement).closest<HTMLElement>("[data-tab]");
      if (button?.dataset.tab !== "baby") return;
      this.holdTimer = window.setTimeout(() => this.openSwitcher(), HOLD_MS);
    });

    // Any way the press can end — including dragging off the button — cancels it.
    for (const ending of ["pointerup", "pointercancel", "pointerleave"]) {
      this.addEventListener(ending, () => window.clearTimeout(this.holdTimer));
    }

    this.syncCurrent();
  }

  protected override subscribe(): void {
    // The bar has to follow `ui.tab`. Without this, `syncCurrent()` runs only
    // from `mount()`, so the highlight is whatever was active when the bar
    // first rendered and never tracks the screen you are actually on.
    this.watch(ui, () => this.syncCurrent());
  }

  protected override disconnect(): void {
    window.clearTimeout(this.holdTimer);
  }

  private openSwitcher(): void {
    this.held = true;
    ui.openSheet({ name: "babies" });
    // If no click follows the release, do not swallow the next tab tap.
    window.setTimeout(() => {
      this.held = false;
    }, 1000);
  }

  private syncCurrent(): void {
    if (this.lastTab === ui.tab) return;
    this.lastTab = ui.tab;
    for (const button of this.querySelectorAll<HTMLElement>("[data-tab]")) {
      if (button.dataset.tab === ui.tab) button.setAttribute("aria-current", "page");
      else button.removeAttribute("aria-current");
    }
  }
}

defineComponent("ll-tabs", LlTabs);
