import { defineComponent, LLComponent } from "../../core/component";
import { LL, ui } from "../../core/ui-store";
import type { DayChangeDetail, HistoryFilter } from "../../core/ui-store";
import type { ChoiceOption } from "../../data/model";
import { store } from "../../data/store";
import { dayStart } from "../../format";
import type { LlChipGroup } from "../ui/ll-chip-group";
import type { LlDayNav } from "../ui/ll-day-nav";
import { LlLogList } from "../ui/ll-log-list";
import { LlPhotoGrid } from "../ui/ll-photo-grid";
import "./ll-history.css";

const DAY = 24 * 60 * 60 * 1000;

const FILTERS: ChoiceOption<HistoryFilter>[] = [
  { id: "all", label: "All" },
  { id: "feed", label: "Feeds", icon: "milk" },
  { id: "sleep", label: "Sleep", icon: "moon" },
  { id: "diaper", label: "Diapers", icon: "droplet" },
  { id: "photo", label: "Photos", icon: "camera" },
];

/** Day-by-day history with kind filters and a photo gallery. */
export class LlHistory extends LLComponent {
  private list: LlLogList | null = null;
  private gallery: LlPhotoGrid | null = null;

  protected template(): string {
    return `
      <h1>History</h1>
      <ll-day-nav data-role="day"></ll-day-nav>
      <ll-chip-group data-role="filters"></ll-chip-group>
      <ll-photo-grid hidden></ll-photo-grid>
      <ll-log-list class="history-log"></ll-log-list>
    `;
  }

  protected mount(): void {
    this.list = this.querySelector<LlLogList>("ll-log-list");
    this.gallery = this.querySelector<LlPhotoGrid>("ll-photo-grid");

    this.querySelector<LlChipGroup>('[data-role="filters"]')?.configure({
      options: FILTERS,
      value: ui.historyFilter,
      toggle: false,
    });
    this.paint();
  }

  protected subscribe(): void {
    this.watch(store);
    this.watch(ui);

    this.addEventListener(LL.dayChange, (event) => {
      const { delta } = (event as CustomEvent<DayChangeDetail>).detail;
      ui.setHistoryDay(ui.historyDay + delta * DAY);
    });

    this.addEventListener("ll-change", (event) => {
      if ((event.target as HTMLElement).dataset.role === "filters") {
        ui.setHistoryFilter((event as CustomEvent<{ value: HistoryFilter | null }>).detail.value ?? "all");
      }
    });
  }

  private isToday(): boolean {
    return ui.historyDay === dayStart(new Date());
  }

  private paint(): void {
    const nav = this.querySelector<LlDayNav>('[data-role="day"]');
    if (nav) {
      nav.setAttribute(
        "label",
        this.isToday()
          ? "Today"
          : new Date(ui.historyDay).toLocaleDateString([], {
              weekday: "short",
              month: "short",
              day: "numeric",
            }),
      );
      nav.toggleAttribute("can-next", !this.isToday());
    }

    const filter = ui.historyFilter;
    const end = ui.historyDay + DAY;
    const entries = store.entries.filter(
      (entry) =>
        entry.occurred_at >= ui.historyDay &&
        entry.occurred_at < end &&
        (filter === "all" || filter === "photo" || entry.kind === filter),
    );

    if (this.list) {
      this.list.hidden = filter === "photo";
      this.list.setEntries(entries, "Nothing logged this day.");
    }
    if (this.gallery) {
      this.gallery.hidden = filter !== "photo";
      this.gallery.setEntries(store.entries.filter((entry) => entry.photo));
    }
  }
}

defineComponent("ll-history", LlHistory);
