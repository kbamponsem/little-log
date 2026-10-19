import { defineComponent, LLComponent } from "../../core/component";
import { copyText } from "../../core/clipboard";
import { ui } from "../../core/ui-store";
import type { Theme } from "../../data/model";
import { THEMES } from "../../data/model";
import { store } from "../../data/store";
import { ic } from "../../icons";
import type { LlChipGroup } from "../ui/ll-chip-group";
import "./ll-settings.css";

/**
 * Settings: everything about *this device* rather than about a child.
 *
 * The device key, the appearance preference and the note about recovery used to
 * live on the Baby screen, where they sat beside the child's name and birthday
 * as though they belonged to it. They do not: the log is grouped around a
 * child, and this is the one page that is not.
 */
export class LlSettings extends LLComponent {
  private idRevealed = false;

  protected template(): string {
    return `
      <div class="settings">
        <header class="settings-head">
          <button class="back" type="button" data-action="close" aria-label="Back">
            ${ic("chevron-left", 22)}
          </button>
          <h1>Settings</h1>
        </header>

        <ll-card class="card" heading="This device">
          <div class="kv">
            <span class="label">Device ID</span>
            <code class="device-id" data-role="device-id"></code>
          </div>
          <div class="chips">
            <button class="chip" type="button" data-action="reveal-id">Show</button>
            <button class="chip" type="button" data-action="copy-id">Copy</button>
          </div>
          <div class="grp">Appearance</div>
          <ll-chip-group data-role="theme"></ll-chip-group>
          <p class="note">
            This device's key is made here on first open and never leaves it,
            which is why there is no password and no sign-in. It cannot be
            recovered, so keep a second device in sync to keep the log safe.
          </p>
        </ll-card>
      </div>
    `;
  }

  protected mount(): void {
    this.querySelector<LlChipGroup>('[data-role="theme"]')?.configure({
      options: THEMES,
      value: store.theme,
      toggle: false,
    });
    this.renderDevice();
  }

  protected subscribe(): void {
    this.watch(store);
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "close") ui.closeOverlay();
      if (action === "reveal-id") {
        this.idRevealed = !this.idRevealed;
        this.renderDevice();
      }
      if (action === "copy-id") void this.copyId(event);
    });
    this.addEventListener("ll-change", (event) => {
      const role = (event.target as HTMLElement).dataset.role;
      const value = (event as CustomEvent<{ value: string | null }>).detail.value;
      if (role === "theme" && value) void store.saveTheme(value as Theme);
    });
  }

  /**
   * Show the device id — the public half of the key. Short by default; the full
   * value is there for comparing two devices, which is the only way to tell one
   * apart from another when there is no account to look at.
   */
  private renderDevice(): void {
    const id = store.deviceId ?? "";
    const slot = this.querySelector<HTMLElement>('[data-role="device-id"]');
    if (slot) {
      slot.textContent = !id
        ? "unavailable"
        : this.idRevealed
          ? id
          : `${id.slice(0, 12)}…${id.slice(-4)}`;
      slot.title = id;
    }
    const button = this.querySelector<HTMLButtonElement>('[data-action="reveal-id"]');
    if (button) button.textContent = this.idRevealed ? "Hide" : "Show";
  }

  private async copyId(event: MouseEvent): Promise<void> {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>(
      "[data-action='copy-id']",
    );
    if (!button || !store.deviceId) return;
    button.textContent = (await copyText(store.deviceId)) ? "Copied ✓" : "Copy failed";
  }
}

defineComponent("ll-settings", LlSettings);
