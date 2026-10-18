import { defineComponent, LLComponent } from "../../core/component";
import { boolAttr } from "../../core/dom";
import type { EntryKind } from "../../data/model";
import { esc } from "../../format";
import { ic } from "../../icons";
import "./ll-tile.css";

/**
 * The big Today action button.
 *
 * `<ll-tile kind="feed" title="Feed" subtitle="No feeds yet" icon="milk"></ll-tile>`
 * Add `running` to pulse a sleep tile.
 * Emits `ll-tile-press` with `{ kind }`.
 */
export class LlTile extends LLComponent {
  protected template(): string {
    const kind = this.getAttribute("kind") ?? "feed";
    const title = esc(this.getAttribute("title") ?? "");
    const subtitle = esc(this.getAttribute("subtitle") ?? "");
    const icon = this.getAttribute("icon") ?? "milk";
    const running = boolAttr(this, "running") ? " run" : "";
    return `
      <button class="tile ${kind}${running}" type="button">
        <span class="em">${ic(icon, 54)}</span>
        <span><span class="t">${title}</span><span class="s">${subtitle}</span></span>
      </button>
    `;
  }

  protected mount(): void {
    this.querySelector("button")?.addEventListener("click", () => {
      const kind = (this.getAttribute("kind") ?? "feed") as EntryKind;
      this.emit("ll-tile-press", { kind });
    });
  }
}

defineComponent("ll-tile", LlTile);
