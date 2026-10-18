import { defineComponent, LLComponent } from "../../core/component";
import { appendSlot } from "../../core/dom";
import { ui } from "../../core/ui-store";
import { store } from "../../data/store";
import { ageStr, ago, dur } from "../../format";
import { LlLogList } from "../ui/ll-log-list";
import { LlStat } from "../ui/ll-stat";
import { LlTile } from "../ui/ll-tile";
import "./ll-today.css";

/** The Today screen: summary stats, three big actions, and the timeline. */
export class LlToday extends LLComponent {
  private list: LlLogList | null = null;

  protected template(): string {
    return `
      <h1>Little Log</h1>
      <p class="sub" data-slot="sub"></p>
      <div class="stats" data-slot="stats"></div>
      <div class="tiles" data-slot="tiles"></div>
      <button class="link" type="button" data-action="past-nap">Log a past nap</button>
      <h2>Today’s timeline</h2>
      <ll-log-list></ll-log-list>
    `;
  }

  protected mount(): void {
    this.list = this.querySelector<LlLogList>("ll-log-list");
    this.renderHead();
    this.renderStats();
    this.renderTiles();
    this.renderLog();
  }

  protected subscribe(): void {
    this.watch(store);
    this.addEventListener("click", (event) => {
      if ((event.target as HTMLElement).closest("[data-action='past-nap']")) {
        ui.openSheet({ name: "sleep" });
      }
    });
    this.addEventListener("ll-tile-press", (event) => {
      void this.onTile((event as CustomEvent<{ kind: "feed" | "sleep" | "diaper" }>).detail.kind);
    });
  }

  private renderHead(): void {
    const baby = store.baby;
    const part = baby.born ? ` · ${ageStr(baby.born)}` : "";
    const date = new Date().toLocaleDateString([], {
      weekday: "long",
      month: "short",
      day: "numeric",
    });
    const sub = this.querySelector<HTMLElement>('[data-slot="sub"]');
    if (sub) sub.textContent = `${baby.name || "Baby"}${part} · ${date}`;
  }

  private renderStats(): void {
    const stats = store.stats();
    appendSlot(
      this,
      "stats",
      stat(String(stats.feeds), "feeds"),
      stat(dur(stats.sleepMs), "slept"),
      stat(String(stats.diapers), "diapers"),
    );
  }

  private renderTiles(): void {
    const nextFeed = store.lastOf("feed");
    const nextDiaper = store.lastOf("diaper");
    const live = store.runningSleep();

    appendSlot(
      this,
      "tiles",
      tile({
        kind: "feed",
        title: "Feed",
        subtitle: nextFeed ? `Last: ${ago(nextFeed.occurred_at)}` : "No feeds yet",
        icon: "milk",
      }),
      tile({
        kind: "sleep",
        title: live ? "Wake up" : "Sleep",
        subtitle: live ? `Asleep for ${dur(Date.now() - live.occurred_at)}` : "Tap when baby falls asleep",
        icon: "moon",
        running: !!live,
      }),
      tile({
        kind: "diaper",
        title: "Diaper",
        subtitle: nextDiaper ? `Last: ${ago(nextDiaper.occurred_at)}` : "No changes yet",
        icon: "droplet",
      }),
    );
  }

  private renderLog(): void {
    this.list?.setEntries(store.today(), "Nothing logged yet today. Tap a big button to start.");
  }

  private async onTile(kind: "feed" | "sleep" | "diaper"): Promise<void> {
    if (kind === "feed") return ui.openSheet({ name: "feed" });
    if (kind === "diaper") return ui.openSheet({ name: "diaper" });
    if (store.runningSleep()) return ui.openSheet({ name: "sleep" });
    try {
      await store.log({ kind: "sleep", occurred_at: Date.now() });
    } catch (error) {
      window.alert(`Could not start sleep: ${error instanceof Error ? error.message : error}`);
    }
  }
}

function stat(value: string, label: string): LlStat {
  const element = new LlStat();
  element.setAttribute("value", value);
  element.setAttribute("label", label);
  return element;
}

function tile(options: {
  kind: "feed" | "sleep" | "diaper";
  title: string;
  subtitle: string;
  icon: string;
  running?: boolean;
}): LlTile {
  const element = new LlTile();
  element.setAttribute("kind", options.kind);
  element.setAttribute("title", options.title);
  element.setAttribute("subtitle", options.subtitle);
  element.setAttribute("icon", options.icon);
  if (options.running) element.setAttribute("running", "");
  return element;
}

defineComponent("ll-today", LlToday);
