import { defineComponent, LLComponent } from "../../core/component";
import { esc } from "../../format";
import "./ll-stat.css";

/** One number in the Today summary row. */
export class LlStat extends LLComponent {
  protected template(): string {
    const value = esc(this.getAttribute("value") ?? "0");
    const label = esc(this.getAttribute("label") ?? "");
    return `<div class="stat"><b aria-live="polite">${value}</b><span>${label}</span></div>`;
  }
}

defineComponent("ll-stat", LlStat);
