import { defineComponent, LLComponent } from "../../core/component";
import { esc } from "../../format";
import { ic } from "../../icons";
import "./ll-bar-chart.css";

export interface BarChartConfig {
  title: string;
  icon: string;
  /** One value per bar. */
  values: number[];
  /** Local-midnight timestamp per bar (for the weekday label). */
  days: number[];
  /** Bar colour, e.g. `var(--feed)`. */
  color: string;
  /** Fraction digits on the value label (0 or 1). */
  decimals?: number;
}

/** A card with a seven-day bar chart. */
export class LlBarChart extends LLComponent {
  private config: BarChartConfig = {
    title: "",
    icon: "",
    values: [],
    days: [],
    color: "var(--feed)",
  };

  configure(config: BarChartConfig): void {
    this.config = config;
    this.render();
  }

  protected template(): string {
    const { title, icon, values, days, color, decimals = 0 } = this.config;
    const max = Math.max(...values, 1);
    const bars = values
      .map((value, index) => {
        const height = (value / max) * 100;
        const label = decimals ? value.toFixed(decimals) : String(value);
        const weekday = new Date(days[index] ?? 0).toLocaleDateString([], { weekday: "narrow" });
        return `
          <div class="bc">
            <div class="bt"><i style="height:${height}%;background:${esc(color)}"></i></div>
            <b>${label}</b>${esc(weekday)}
          </div>
        `;
      })
      .join("");
    return `
      <section class="card">
        <h2 class="ch">${ic(icon, 22)}${esc(title)}</h2>
        <div class="bars">${bars}</div>
      </section>
    `;
  }
}

defineComponent("ll-bar-chart", LlBarChart);
