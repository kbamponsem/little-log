import { defineComponent } from "../../core/component";
import { LLSheet } from "../../core/sheet";
import { SLEEP_LOCATIONS, WAKE_MOODS, newEntry } from "../../data/model";
import type { SleepLocation, WakeMood } from "../../data/model";
import { capNow, setT, tv } from "../../format";
import type { LlChipGroup } from "../ui/ll-chip-group";

const HALF_HOUR = 30 * 60 * 1000;

/** Start a nap, log a past one, or wake the baby up. */
export class LlSleepSheet extends LLSheet {
  private location: SleepLocation | null = null;
  private mood: WakeMood | null = null;

  protected template(): string {
    const live = this.store.runningSleep();
    const started = live ? live.occurred_at : Date.now() - HALF_HOUR;
    const title = live ? "Baby woke up" : "Log a past nap";
    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>${title}</h3>
      <div class="two">
        <ll-field name="from" label="Fell asleep" type="time" value="${tv(started)}"></ll-field>
        <ll-field name="to" label="Woke up" type="time" value="${tv(Date.now())}"></ll-field>
      </div>
      <div class="grp">Where did they sleep?</div>
      <ll-chip-group data-role="location"></ll-chip-group>
      <div class="grp">Waking mood</div>
      <ll-chip-group data-role="mood"></ll-chip-group>
      <ll-field name="note" label="Notes" multiline></ll-field>
      <p class="err" hidden></p>
      <button class="go" type="button" data-action="save">Save nap</button>
    `;
  }

  protected mount(): void {
    this.querySelector<LlChipGroup>('[data-role="location"]')?.configure({
      options: SLEEP_LOCATIONS,
      value: null,
    });
    this.querySelector<LlChipGroup>('[data-role="mood"]')?.configure({
      options: WAKE_MOODS,
      value: null,
    });
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "close") this.close();
      if (action === "save") void this.save();
    });

    this.addEventListener("ll-change", (event) => {
      const role = (event.target as HTMLElement).dataset.role;
      const value = (event as CustomEvent<{ value: string | null }>).detail.value;
      if (role === "location") this.location = value as SleepLocation | null;
      if (role === "mood") this.mood = value as WakeMood | null;
    });
  }

  private async save(): Promise<void> {
    this.error(null);
    const live = this.store.runningSleep();
    const from = this.field("from") || tv(live ? live.occurred_at : Date.now() - HALF_HOUR);
    const to = this.field("to") || tv(Date.now());
    const note = this.field("note");

    let started = setT(live ? live.occurred_at : Date.now(), from);
    const ended = capNow(setT(Date.now(), to));
    if (ended <= started) started -= 24 * 60 * 60 * 1000;

    const noteValue = note || null;
    const target = live
      ? { ...live, occurred_at: started, ended_at: ended, updated_at: Date.now() }
      : newEntry({ kind: "sleep", occurred_at: started, ended_at: ended });
    target.sleep_location = this.location;
    target.wake_mood = this.mood;
    target.note = noteValue;

    try {
      await this.store.saveEntry(target);
      this.close();
    } catch (error) {
      this.error(this.message(error));
    }
  }
}

defineComponent("ll-sleep-sheet", LlSleepSheet);
