import { defineComponent, LLComponent } from "../../core/component";
import { TABS } from "../../data/model";
import type { TabName } from "../../data/model";
import { ic } from "../../icons";
import { ui } from "../../core/ui-store";
import "./ll-tabs.css";

/** The fixed bottom tab bar. Switches `ui.tab`. */
export class LlTabs extends LLComponent {
  private lastTab: TabName | null = null;

  protected template(): string {
    return TABS.map(
      (tab) =>
        `<button class="tab" type="button" data-tab="${tab.id}"><span>${ic(tab.icon, 24)}</span>${tab.label}</button>`,
    ).join("");
  }

  protected mount(): void {
    this.addEventListener("click", (event) => {
      const button = (event.target as HTMLElement).closest<HTMLElement>("[data-tab]");
      if (button) ui.go(button.dataset.tab as TabName);
    });
    this.syncCurrent();
  }

  protected subscribe(): void {
    this.watch(ui, () => this.syncCurrent());
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
