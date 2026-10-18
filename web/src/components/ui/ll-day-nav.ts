import { defineComponent, LLComponent } from "../../core/component";
import { boolAttr, strAttr } from "../../core/dom";
import { esc } from "../../format";
import { ic } from "../../icons";
import { LL } from "../../core/ui-store";
import "./ll-day-nav.css";

/**
 * `<ll-day-nav label="Today" can-next>` — previous/next day.
 * Emits `ll-day-change` with `{ delta }`; the screen applies it to its cursor.
 */
export class LlDayNav extends LLComponent {
  static observedAttributes = ["label", "can-next"];

  protected template(): string {
    const canNext = boolAttr(this, "can-next");
    return `
      <div class="daynav">
        <button class="chip" type="button" data-delta="-1" aria-label="Previous day">${ic("chevron-left", 22)}</button>
        <b>${esc(strAttr(this, "label"))}</b>
        <button class="chip" type="button" data-delta="1" aria-label="Next day"${canNext ? "" : ' aria-disabled="true"'}>${ic("chevron-right", 22)}</button>
      </div>
    `;
  }

  protected mount(): void {
    for (const button of this.querySelectorAll<HTMLButtonElement>("[data-delta]")) {
      button.addEventListener("click", () => {
        const delta = Number(button.dataset.delta);
        if (delta > 0 && !boolAttr(this, "can-next")) return; // already on today
        this.emit(LL.dayChange, { delta });
      });
    }
  }

  attributeChangedCallback(): void {
    this.rerender();
  }
}

defineComponent("ll-day-nav", LlDayNav);
