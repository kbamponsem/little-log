import { defineComponent, LLComponent } from "../../core/component";
import { fromHtml } from "../../core/dom";
import { ALERT_POOP_COLORS, POOP_COLORS, labelOf } from "../../data/model";
import type { PoopColor } from "../../data/model";
import { store } from "../../data/store";
import { dur, esc } from "../../format";
import { ic } from "../../icons";
import { LlBarChart } from "../ui/ll-bar-chart";
import "./ll-trends.css";

const DAY = 24 * 60 * 60 * 1000;
const LAST_7 = [0, 1, 2, 3, 4, 5, 6];

/** Seven-day trends: feed gaps, per-day counts, and poop colours. */
export class LlTrends extends LLComponent {
  protected template(): string {
    return `<h1>Trends</h1><p class="sub">Last 7 days</p><div class="trend-cards" data-slot="cards"></div>`;
  }

  protected mount(): void {
    const days = LAST_7.map((offset) => {
      const date = new Date();
      date.setHours(0, 0, 0, 0);
      date.setDate(date.getDate() - 6 + offset);
      return date.getTime();
    });
    const now = Date.now();
    const perDay = (select: (entries: typeof store.entries) => number): number[] =>
      days.map((start) =>
        select(store.entries.filter((entry) => entry.occurred_at >= start && entry.occurred_at < start + DAY)),
      );

    const feeds = perDay((entries) => entries.filter((entry) => entry.kind === "feed").length);
    const hours = perDay(
      (entries) =>
        entries
          .filter((entry) => entry.kind === "sleep")
          .reduce((total, entry) => total + ((entry.ended_at ?? now) - entry.occurred_at), 0) / 3_600_000,
    );
    const diapers = perDay((entries) => entries.filter((entry) => entry.kind === "diaper").length);

    const feedTimes = store.entries
      .filter((entry) => entry.kind === "feed" && entry.occurred_at >= days[0])
      .map((entry) => entry.occurred_at)
      .sort((a, b) => a - b);
    const gap =
      feedTimes.length > 1
        ? dur((feedTimes[feedTimes.length - 1] - feedTimes[0]) / (feedTimes.length - 1))
        : null;

    const poop = new Map<PoopColor, number>();
    for (const entry of store.entries) {
      if (
        entry.kind === "diaper" &&
        entry.occurred_at >= days[0] &&
        (entry.diaper_type === "poop" || entry.diaper_type === "both") &&
        entry.poop_color
      ) {
        poop.set(entry.poop_color, (poop.get(entry.poop_color) ?? 0) + 1);
      }
    }
    const flagged = ALERT_POOP_COLORS.some((color) => poop.has(color));

    const cards = this.querySelector<HTMLElement>('[data-slot="cards"]');
    if (!cards) return;
    cards.replaceChildren(
      fromHtml(gapCard(gap)),
      chart("Feeds per day", "milk", feeds, days, "var(--feed)"),
      chart("Hours slept per day", "moon", hours, days, "var(--sleep)", 1),
      chart("Diapers per day", "droplet", diapers, days, "var(--diaper)"),
      fromHtml(
        `
        <section class="card">
          <h2 class="ch">Poop colours</h2>
          <div class="chips">${poop.size ? poopChips(poop) : `<span class="note">No poop colours logged yet.</span>`}</div>
          ${flagged ? `<p class="note">Red, black or pale stools are worth mentioning to your pediatrician.</p>` : ""}
        </section>
        `,
      ),
    );
  }

  protected subscribe(): void {
    this.watch(store);
  }
}

/** The average gap between feeds, or an empty state until there are two. */
function gapCard(gap: string | null): string {
  const body = gap
    ? `<b class="big-number">${esc(gap)}</b><small class="note">on average</small>`
    : `
      <div class="empty-state">
        <span class="empty-state-icon">${ic("clock", 24)}</span>
        <span>
          <b>Not enough data yet</b>
          <small class="note">Log two or more feeds and the average gap appears here.</small>
        </span>
      </div>
    `;
  return `<section class="card"><h2 class="ch">Time between feeds</h2>${body}</section>`;
}

function chart(
  title: string,
  icon: string,
  values: number[],
  days: number[],
  color: string,
  decimals = 0,
): LlBarChart {
  const element = new LlBarChart();
  element.configure({ title, icon, values, days, color, decimals });
  return element;
}

/** Chip list of poop colours, most common first. */
function poopChips(counts: Map<PoopColor, number>): string {
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([color, count]) => {
      const dot = POOP_COLORS.find((option) => option.id === color)?.dot ?? "transparent";
      return (
        `<span class="chip"><span class="dot" style="background:${esc(dot)}"></span>${esc(labelOf(POOP_COLORS, color))} ×${count}</span>`
      );
    })
    .join("");
}

defineComponent("ll-trends", LlTrends);
