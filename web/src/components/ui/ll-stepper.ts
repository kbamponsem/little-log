import { defineComponent, LLComponent } from "../../core/component";
import { $ } from "../../core/dom";
import type { AmountUnit } from "../../data/model";
import "./ll-stepper.css";

/** Amount picker: − / value + unit / +. Emits `ll-step` with `{ value }`. */
export class LlStepper extends LLComponent {
  private amount = 0;
  private unit: AmountUnit = "min";

  configure(value: number, unit: AmountUnit): void {
    this.amount = value;
    this.unit = unit;
    this.render();
  }

  protected template(): string {
    return `
      <button class="step-btn" type="button" data-step="-1" aria-label="Less">−</button>
      <output>${this.amount} ${this.unit}</output>
      <button class="step-btn" type="button" data-step="1" aria-label="More">+</button>
    `;
  }

  protected mount(): void {
    for (const button of this.querySelectorAll<HTMLButtonElement>("[data-step]")) {
      button.addEventListener("click", () => this.step(Number(button.dataset.step)));
    }
  }

  private step(direction: number): void {
    const delta = this.unit === "ml" ? 10 : 1;
    this.amount = Math.max(0, this.amount + direction * delta);
    const output = $("output", this);
    if (output) output.textContent = `${this.amount} ${this.unit}`;
    this.emit("ll-step", { value: this.amount });
  }
}

defineComponent("ll-stepper", LlStepper);
