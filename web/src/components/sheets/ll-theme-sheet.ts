import { defineComponent } from "../../core/component";
import { LLSheet } from "../../core/sheet";
import type { Theme } from "../../data/model";
import { THEMES } from "../../data/model";
import type { LlChipGroup } from "../ui/ll-chip-group";
import "./ll-theme-sheet.css";

/**
 * Appearance: which theme this device uses.
 *
 * Opened from the cog on the intro and on Today, so the theme can be set before
 * there is anything to log. It is a device preference (see
 * `AppStore.saveTheme`), so it never travels with the log.
 *
 * Picking one deliberately does not close the sheet: the page behind restyles
 * immediately, so the three options are meant to be tried in turn. Close it
 * with the ×, the scrim, or Escape.
 */
export class LlThemeSheet extends LLSheet {
  /** Keyboard users land on the first theme. */
  autofocusSelector = "ll-chip-group .chip";

  protected template(): string {
    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>Appearance</h3>
      <ll-chip-group data-role="theme"></ll-chip-group>
      <p class="note">
        A preference for this device only, so it never travels with the log.
      </p>
    `;
  }

  protected mount(): void {
    this.querySelector<LlChipGroup>('[data-role="theme"]')?.configure({
      options: THEMES,
      value: this.store.theme,
      toggle: false,
    });
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      if ((event.target as HTMLElement).closest("[data-action='close']")) this.close();
    });

    this.addEventListener("ll-change", (event) => {
      const role = (event.target as HTMLElement).dataset.role;
      const value = (event as CustomEvent<{ value: string | null }>).detail.value;
      if (role === "theme" && value) void this.store.saveTheme(value as Theme);
    });
  }
}

defineComponent("ll-theme-sheet", LlThemeSheet);
