import { defineComponent, LLComponent } from "../../core/component";
import { $$ } from "../../core/dom";
import type { ChoiceOption } from "../../data/model";
import { esc } from "../../format";
import type { LlChip } from "./ll-chip";
import "./ll-chip-group.css";

export type ChipVariant = "plain" | "big" | "card" | "mood";

export interface ChipGroupConfig {
  options: readonly ChoiceOption[];
  value?: string | null;
  variant?: ChipVariant;
  /** Allow tapping the selected chip to clear it (default true). */
  toggle?: boolean;
}

/**
 * A row/grid of `ll-chip`s with single-select behaviour.
 *
 * Configured imperatively (option lists are not attribute-shaped):
 *
 *   const group = document.createElement("ll-chip-group");
 *   group.configure({ options: PEE_SHADES, value: null });
 *   group.addEventListener("ll-change", (e) => { ... e.detail.value ... });
 *
 * Emits `ll-change` with `{ value: string | null }`.
 */
export class LlChipGroup extends LLComponent {
  private options: readonly ChoiceOption[] = [];
  private value: string | null = null;
  private variant: ChipVariant = "plain";
  private toggle = true;

  configure(config: ChipGroupConfig): void {
    this.options = config.options;
    this.value = config.value ?? null;
    this.variant = config.variant ?? "plain";
    this.toggle = config.toggle ?? true;
    this.render();
  }

  protected template(): string {
    const layout = this.variant === "card" || this.variant === "mood" ? "cards" : "chips";
    const chips = this.options.map((option) => this.markup(option)).join("");
    return `<div class="${layout}" role="group">${chips}</div>`;
  }

  protected subscribe(): void {
    this.addEventListener("ll-chip-press", (event) => {
      this.select((event as CustomEvent<{ value: string }>).detail.value);
    });
  }

  private markup(option: ChoiceOption): string {
    const attrs = [
      `value="${esc(option.id)}"`,
      `label="${esc(option.label)}"`,
      `variant="${this.variant}"`,
      this.value === option.id ? "pressed" : "",
      option.icon ? `icon="${esc(option.icon)}"` : "",
      option.icons?.length ? `icons="${esc(option.icons.join(","))}"` : "",
      option.dot ? `dot="${esc(option.dot)}"` : "",
    ];
    return `<ll-chip ${attrs.filter(Boolean).join(" ")}></ll-chip>`;
  }

  private select(next: string): void {
    this.value = this.toggle && this.value === next ? null : next;
    for (const chip of $$<LlChip & HTMLElement>("ll-chip", this)) {
      chip.setPressed(chip.value === this.value);
    }
    this.emit("ll-change", { value: this.value });
  }
}

defineComponent("ll-chip-group", LlChipGroup);
