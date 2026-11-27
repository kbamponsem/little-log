import { defineComponent, LLComponent } from "../../core/component";
import { $, $$ } from "../../core/dom";
import type { AmountUnit } from "../../data/model";
import "./ll-amount-picker.css";

interface AmountRange {
  min: number;
  max: number;
  step: number;
}

/**
 * The core rejects values outside these ranges; the picker never offers them.
 * Keep in step with `MAX_BREAST_MINUTES` and `MAX_BOTTLE_ML` in
 * `crates/core/src/validate.rs`.
 */
const RANGES: Record<AmountUnit, AmountRange> = {
  min: { min: 0, max: 180, step: 1 },
  ml: { min: 0, max: 2000, step: 10 },
};

/**
 * A horizontal, snapping amount picker.
 *
 * Configured imperatively:
 *
 *   picker.configure(90, "ml");
 *
 * Emits `ll-step` with `{ value }` whenever the selected number changes.
 * The whole strip is one focusable `spinbutton`: touch users swipe, keyboard
 * users use the arrow/Page/Home/End keys, and assistive tech gets the value
 * and its bounds from the ARIA attributes.
 */
export class LlAmountPicker extends LLComponent {
  private value = 0;
  private unit: AmountUnit = "min";
  private frame = 0;

  configure(value: number, unit: AmountUnit): void {
    this.unit = unit;
    this.value = this.snap(value);
    this.render();
  }

  protected template(): string {
    const range = RANGES[this.unit];
    const values = this.values();
    const selected = this.indexOf(this.value);
    const items = values
      .map(
        (value, index) =>
          `<span class="picker-item${index === selected ? " is-selected" : ""}" data-value="${value}" aria-hidden="true">${value}</span>`,
      )
      .join("");

    return `
      <div class="picker">
        <div class="picker-scroll" data-role="scroll" tabindex="0" role="spinbutton"
          aria-label="${this.unit === "ml" ? "Feed amount in millilitres" : "Feed amount in minutes"}"
          aria-valuemin="${range.min}"
          aria-valuemax="${range.max}"
          aria-valuenow="${this.value}"
          aria-valuetext="${this.valueText(this.value)}">
          ${items}
        </div>
      </div>
    `;
  }

  protected mount(): void {
    const scroller = this.scroller;
    if (!scroller) return;
    scroller.addEventListener("scroll", this.onScroll, { passive: true });
    scroller.addEventListener("keydown", this.onKeyDown);
    // Center the initial value once the browser has laid the strip out.
    requestAnimationFrame(() => {
      if (this.isConnected) this.center(this.indexOf(this.value), "auto");
    });
  }

  protected override disconnect(): void {
    if (this.frame) cancelAnimationFrame(this.frame);
    this.frame = 0;
    super.disconnect();
  }

  private onScroll = (): void => {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      const width = this.itemWidth();
      if (!width) return;
      this.select(Math.round((this.scroller?.scrollLeft ?? 0) / width), false);
    });
  };

  private onKeyDown = (event: KeyboardEvent): void => {
    const index = this.indexOf(this.value);
    let next = index;
    switch (event.key) {
      case "ArrowLeft":
      case "ArrowUp":
        next = index - 1;
        break;
      case "ArrowRight":
      case "ArrowDown":
        next = index + 1;
        break;
      case "PageUp":
        next = index - 5;
        break;
      case "PageDown":
        next = index + 5;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = this.values().length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    this.select(next, true);
  };

  /** Select `index` and optionally move the strip so that item is centred. */
  private select(index: number, scroll: boolean): void {
    const values = this.values();
    const next = Math.max(0, Math.min(values.length - 1, index));
    const changed = values[next] !== this.value;
    this.value = values[next];
    this.updateSelected(next);
    this.updateAria();
    if (scroll) this.center(next, "smooth");
    if (changed) this.emit("ll-step", { value: this.value });
  }

  private updateSelected(index: number): void {
    $$<HTMLElement>(".picker-item", this).forEach((item, itemIndex) => {
      item.classList.toggle("is-selected", itemIndex === index);
    });
  }

  private updateAria(): void {
    const scroller = this.scroller;
    if (!scroller) return;
    scroller.setAttribute("aria-valuenow", String(this.value));
    scroller.setAttribute("aria-valuetext", this.valueText(this.value));
  }

  private center(index: number, behavior: ScrollBehavior): void {
    const scroller = this.scroller;
    const width = this.itemWidth();
    if (!scroller || !width) return;
    scroller.scrollTo({ left: index * width, behavior });
  }

  private get scroller(): HTMLElement | null {
    return $<HTMLElement>('[data-role="scroll"]', this);
  }

  private itemWidth(): number {
    return $<HTMLElement>(".picker-item", this)?.getBoundingClientRect().width ?? 0;
  }

  private values(): number[] {
    const { min, max, step } = RANGES[this.unit];
    const values: number[] = [];
    for (let value = min; value <= max; value += step) values.push(value);
    return values;
  }

  private indexOf(value: number): number {
    const { min, step } = RANGES[this.unit];
    return Math.round((value - min) / step);
  }

  private snap(value: number): number {
    const { min, max, step } = RANGES[this.unit];
    const clamped = Math.max(min, Math.min(max, value));
    return min + Math.round((clamped - min) / step) * step;
  }

  private valueText(value: number): string {
    if (this.unit === "ml") return `${value} millilitres`;
    return `${value} ${value === 1 ? "minute" : "minutes"}`;
  }
}

defineComponent("ll-amount-picker", LlAmountPicker);
