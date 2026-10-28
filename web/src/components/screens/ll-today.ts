import { defineComponent, LLComponent } from "../../core/component";
import { appendSlot } from "../../core/dom";
import { ui } from "../../core/ui-store";
import { store } from "../../data/store";
import { ageStr, ago, dur, entryTitle } from "../../format";
import { ic } from "../../icons";
import { LlLogList } from "../ui/ll-log-list";
import { LlStat } from "../ui/ll-stat";
import { LlTile } from "../ui/ll-tile";
import "./ll-today.css";

/** The Today screen: summary stats, four big actions, and the timeline. */
export class LlToday extends LLComponent {
  private list: LlLogList | null = null;
  private checkinTimer = 0;

  protected template(): string {
    return `
      <header class="today-head">
        <h1 data-slot="title">Little Log</h1>
        <button class="cog" type="button" data-action="settings" aria-label="Settings">
          ${ic("settings", 28)}
        </button>
      </header>
      <p class="sub" data-slot="sub"></p>
      <div class="stats" data-slot="stats"></div>
      <button class="checkin" type="button" data-slot="checkin" data-action="mood" hidden>
        <span class="checkin-copy">
          <span class="checkin-icon">${ic("heart", 26)}</span>
          <span>
            <b>Daily check-in</b>
            <small>How has baby felt today?</small>
          </span>
        </span>
        <span class="chip">Log mood</span>
      </button>
      <div class="tiles" data-slot="tiles"></div>
      <div class="links">
        <button class="link" type="button" data-action="past-nap">Log a past nap</button>
      </div>
      <h2>Today’s timeline</h2>
      <ll-log-list></ll-log-list>
    `;
  }

  protected mount(): void {
    this.list = this.querySelector<LlLogList>("ll-log-list");
    this.renderHead();
    this.renderStats();
    this.renderTiles();
    this.renderCheckin();
    this.renderLog();
  }

  protected subscribe(): void {
    this.watch(store);
    this.checkinTimer = window.setInterval(() => this.renderCheckin(), 30_000);
    this.addEventListener("click", (event) => {
      const target = event.target as HTMLElement;
      if (target.closest("[data-action='past-nap']")) {
        ui.openSheet({ name: "sleep" });
        return;
      }
      if (target.closest("[data-action='mood']")) {
        ui.openSheet({ name: "mood" });
        return;
      }
      if (target.closest("[data-action='settings']")) {
        ui.openOverlay("settings");
        return;
      }
    });
    this.addEventListener("ll-tile-press", (event) => {
      void this.onTile(
        (event as CustomEvent<{ kind: "feed" | "sleep" | "diaper" | "mood" }>).detail.kind,
      );
    });
  }

  protected override disconnect(): void {
    window.clearInterval(this.checkinTimer);
    super.disconnect();
  }

  private renderHead(): void {
    const baby = store.baby;
    const date = new Date().toLocaleDateString([], {
      weekday: "long",
      month: "short",
      day: "numeric",
    });
    const title = this.querySelector<HTMLElement>('[data-slot="title"]');
    if (title) title.textContent = baby.name || "Baby";
    const sub = this.querySelector<HTMLElement>('[data-slot="sub"]');
    if (sub) sub.textContent = `${baby.born ? ageStr(baby.born) : "No birthday yet"} · ${date}`;
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
    const lastMood = store.lastOf("mood");
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
      tile({
        kind: "mood",
        title: "Mood",
        subtitle: lastMood
          ? `Last: ${entryTitle(lastMood)} · ${ago(lastMood.occurred_at)}`
          : "How was the day?",
        icon: "smile",
      }),
    );
  }

  private renderCheckin(): void {
    const card = this.querySelector<HTMLElement>('[data-slot="checkin"]');
    if (card) card.hidden = !store.moodDue();
  }

  private renderLog(): void {
    this.list?.setEntries(store.today(), "Nothing logged yet today. Tap a big button to start.");
  }

  private async onTile(kind: "feed" | "sleep" | "diaper" | "mood"): Promise<void> {
    if (kind === "feed") return ui.openSheet({ name: "feed" });
    if (kind === "diaper") return ui.openSheet({ name: "diaper" });
    if (kind === "mood") return ui.openSheet({ name: "mood" });
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
  kind: "feed" | "sleep" | "diaper" | "mood";
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
