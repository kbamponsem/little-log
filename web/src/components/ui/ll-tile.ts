import { defineComponent, LLComponent } from "../../core/component";
import { boolAttr } from "../../core/dom";
import type { EntryKind } from "../../data/model";
import { clock, esc } from "../../format";
import { ic } from "../../icons";
import "./ll-tile.css";

/**
 * The big Today action button.
 *
 * `<ll-tile kind="feed" title="Feed" subtitle="No feeds yet" icon="milk"></ll-tile>`
 * Add `running` to pulse a sleep tile.
 *
 * For a running feed, pass `timer-start` (epoch millis) and the tile keeps its
 * own clock up to date, so today does not have to re-render every second:
 * `<ll-tile ... timer-start="1795772510710">`.
 *
 * Emits `ll-tile-press` with `{ kind }`.
 */
export class LlTile extends LLComponent {
  private tick = 0;

  protected template(): string {
    const kind = this.getAttribute("kind") ?? "feed";
    const title = esc(this.getAttribute("title") ?? "");
    const icon = this.getAttribute("icon") ?? "milk";
    const running = boolAttr(this, "running") ? " run" : "";
    const timerStart = Number(this.getAttribute("timer-start") ?? 0);
    const ticking = timerStart > 0 ? " ticking" : "";
    const subtitle =
      timerStart > 0
        ? `<span class="s timer" data-slot="timer">${clock(Date.now() - timerStart)}</span>`
        : `<span class="s">${esc(this.getAttribute("subtitle") ?? "")}</span>`;

    return `
      <button class="tile ${kind}${running}" type="button">
        <span class="em${ticking}">${ic(icon, 54)}</span>
        <span><span class="t">${title}</span>${subtitle}</span>
      </button>
    `;
  }

  protected mount(): void {
    const timerStart = Number(this.getAttribute("timer-start") ?? 0);
    if (timerStart > 0) {
      this.tick = window.setInterval(() => {
        const slot = this.querySelector<HTMLElement>('[data-slot="timer"]');
        if (slot) slot.textContent = clock(Date.now() - timerStart);
      }, 1000);
    }

    this.querySelector("button")?.addEventListener("click", () => {
      const kind = (this.getAttribute("kind") ?? "feed") as EntryKind;
      this.emit("ll-tile-press", { kind });
    });
  }

  protected override disconnect(): void {
    window.clearInterval(this.tick);
    super.disconnect();
  }
}

defineComponent("ll-tile", LlTile);
