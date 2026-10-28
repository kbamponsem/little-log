import { defineComponent, LLComponent } from "../../core/component";
import { $ } from "../../core/dom";
import { esc } from "../../format";
import { ic } from "../../icons";
import "./ll-chip.css";

/**
 * A single pill / card button.
 *
 * `<ll-chip value="pee" label="Pee" variant="big" pressed></ll-chip>`
 *
 * Attributes: `value`, `label`, `variant` (plain | big | card), `pressed`,
 * `icon`, `icons` (comma separated), `dot` (colour).
 * Emits `ll-chip-press` with `{ value }`.
 */
export class LlChip extends LLComponent {
  protected template(): string {
    const variant = this.getAttribute("variant") ?? "plain";
    const pressed = this.hasAttribute("pressed");
    const value = this.getAttribute("value") ?? "";
    const label = esc(this.getAttribute("label") ?? "");
    const icon = this.getAttribute("icon");
    const icons = (this.getAttribute("icons") ?? "").split(",").filter(Boolean);
    const dot = this.getAttribute("dot");

    const size = variant === "card" || variant === "mood" ? 40 : variant === "big" ? 34 : 18;
    const pieces: string[] = [];
    if (icons.length) pieces.push(`<span class="ce">${icons.map((n) => ic(n, size)).join("")}</span>`);
    else if (icon) pieces.push(`<span class="ce">${ic(icon, size)}</span>`);
    if (dot) pieces.push(`<span class="dot" style="background:${esc(dot)}"></span>`);

    const classes = ["chip"];
    if (variant === "big") classes.push("big");
    if (variant === "card") classes.push("fc");
    if (variant === "mood") classes.push("mood");

    return `<button class="${classes.join(" ")}" type="button" data-value="${esc(value)}" aria-pressed="${pressed}">${pieces.join("")}${label}</button>`;
  }

  protected mount(): void {
    const button = $("button", this);
    button?.addEventListener("click", () => {
      this.emit("ll-chip-press", { value: this.value });
    });
  }

  get value(): string {
    return this.getAttribute("value") ?? "";
  }

  /** Update the pressed state in place, without a full re-render. */
  setPressed(pressed: boolean): void {
    this.toggleAttribute("pressed", pressed);
    $("button", this)?.setAttribute("aria-pressed", String(pressed));
  }
}

defineComponent("ll-chip", LlChip);
