import { defineComponent, LLComponent } from "../../core/component";
import { esc } from "../../format";
import "./ll-button.css";

export type ButtonVariant = "primary" | "danger" | "link" | "chip" | "big";

const CLASSES: Record<ButtonVariant, string> = {
  primary: "go",
  danger: "del",
  link: "link",
  chip: "chip",
  big: "chip big",
};

/**
 * A styled button.
 *
 * `<ll-button variant="primary" label="Log feeding now"></ll-button>`
 * Variants: primary (.go), danger (.del), link (.link), chip, big.
 * Clicks bubble natively — just listen for `click`.
 */
export class LlButton extends LLComponent {
  static observedAttributes = ["label", "variant", "disabled"];

  protected template(): string {
    const variant = (this.getAttribute("variant") ?? "chip") as ButtonVariant;
    const label = esc(this.getAttribute("label") ?? "");
    const disabled = this.hasAttribute("disabled") ? " disabled" : "";
    const classes = CLASSES[variant] ?? CLASSES.chip;
    return `<button class="${classes}" type="button"${disabled}>${label}</button>`;
  }

  attributeChangedCallback(): void {
    this.rerender();
  }

  /** Swap the label in place (used for "Copied ✓" feedback). */
  setLabel(label: string): void {
    this.setAttribute("label", label);
  }
}

defineComponent("ll-button", LlButton);
