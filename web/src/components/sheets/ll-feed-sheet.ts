import { LLSheet } from "../../core/sheet";
import type { LlChipGroup } from "../ui/ll-chip-group";
import type { LlAmountPicker } from "../ui/ll-amount-picker";
import { defineComponent } from "../../core/component";
import { capNow, clock, setT, tv } from "../../format";
import { FEED_KINDS, labelOf } from "../../data/model";
import type { AmountUnit, Entry, FeedKind } from "../../data/model";
import "./ll-feed-sheet.css";

/** Start, stop or log a feed. Breast left is the default, as in the prototype. */
export class LlFeedSheet extends LLSheet {
  private feedKind: FeedKind = "breast_left";
  private amount = 10;
  private unit: AmountUnit = "min";
  private picker: LlAmountPicker | null = null;
  private label: HTMLElement | null = null;
  private tick = 0;

  protected template(): string {
    const live = this.store.runningFeed();
    if (live) {
      return `
        <button class="x" type="button" data-action="close" aria-label="Close">×</button>
        <h3>Feeding</h3>
        <p class="feed-live">
          ${labelOf(FEED_KINDS, live.feed_kind)} ·
          <span data-slot="elapsed">${clock(Date.now() - live.occurred_at)}</span>
        </p>
        <p class="err" hidden></p>
        <button class="go" type="button" data-action="stop">Stop feeding</button>
      `;
    }

    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>Feeding</h3>
      <div class="grp">What kind?</div>
      <ll-chip-group data-role="kinds"></ll-chip-group>
      <div class="grp" data-slot="amount-label">Minutes</div>
      <ll-amount-picker data-role="picker"></ll-amount-picker>
      <ll-field name="when" label="When" type="time" value="${tv(Date.now())}"></ll-field>
      <ll-field name="note" label="Notes" multiline placeholder="Anything worth remembering?"></ll-field>
      <p class="err" hidden></p>
      <div class="feed-actions">
        <button class="go" type="button" data-action="start">Start feeding</button>
        <button class="feed-secondary" type="button" data-action="save">Log feeding now</button>
      </div>
    `;
  }

  protected mount(): void {
    const live = this.store.runningFeed();
    if (live) {
      // The sheet can stay open on a running feed, so keep the elapsed time honest.
      this.tick = window.setInterval(() => {
        const slot = this.querySelector<HTMLElement>('[data-slot="elapsed"]');
        if (slot) slot.textContent = clock(Date.now() - live.occurred_at);
      }, 1000);
      return;
    }

    this.picker = this.querySelector<LlAmountPicker>('[data-role="picker"]');
    this.label = this.querySelector<HTMLElement>('[data-slot="amount-label"]');

    this.querySelector<LlChipGroup>('[data-role="kinds"]')?.configure({
      options: FEED_KINDS,
      value: this.feedKind,
      variant: "card",
      // Feed kinds are a required single choice — no tap-to-clear.
      toggle: false,
    });

    this.picker?.addEventListener("ll-step", (event) => {
      this.amount = (event as CustomEvent<{ value: number }>).detail.value;
    });

    this.applyKind();
  }

  protected override disconnect(): void {
    window.clearInterval(this.tick);
    super.disconnect();
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "close") this.close();
      if (action === "save") void this.save();
      if (action === "start") void this.start();
      if (action === "stop") void this.stop();
    });

    this.addEventListener("ll-change", (event) => {
      this.feedKind = (event as CustomEvent<{ value: FeedKind }>).detail.value;
      this.applyKind();
    });
  }

  /** Show/hide the picker and keep its unit in sync with the feed kind. */
  private applyKind(): void {
    const bottle = this.feedKind === "bottle";
    const breast = this.feedKind === "breast_left" || this.feedKind === "breast_right";
    const withAmount = bottle || breast;

    if (bottle && this.unit !== "ml") {
      this.unit = "ml";
      this.amount = 90;
    } else if (breast && this.unit !== "min") {
      this.unit = "min";
      this.amount = 10;
    }

    if (this.label) {
      this.label.hidden = !withAmount;
      this.label.textContent = bottle ? "Amount (ml)" : "Minutes";
    }
    if (this.picker) {
      this.picker.hidden = !withAmount;
      this.picker.configure(this.amount, this.unit);
    }
  }

  /** Start a feed now. The end time, and therefore the duration, comes later. */
  private async start(): Promise<void> {
    this.error(null);
    if (this.store.runningFeed()) {
      this.error("A feed is already running — stop it first.");
      return;
    }

    try {
      await this.store.log({
        kind: "feed",
        occurred_at: Date.now(),
        feed_kind: this.feedKind,
        note: this.field("note") || null,
      });
      this.close();
    } catch (error) {
      this.error(this.message(error));
    }
  }

  /** Stop the running feed; its duration is derived from the two timestamps. */
  private async stop(): Promise<void> {
    this.error(null);
    const live = this.store.runningFeed();
    if (!live) {
      this.close();
      return;
    }

    try {
      const stopped: Entry = { ...live, ended_at: capNow(Date.now()), updated_at: Date.now() };
      await this.store.saveEntry(stopped);
      this.close();
    } catch (error) {
      this.error(this.message(error));
    }
  }

  private async save(): Promise<void> {
    this.error(null);
    const when = this.field("when") || tv(Date.now());
    const note = this.field("note");
    const bottle = this.feedKind === "bottle";
    const breast = this.feedKind === "breast_left" || this.feedKind === "breast_right";
    const withAmount = bottle || breast;

    // A completed feed needs an end time: without one the session is still
    // running and the Today tile starts a timer. Breast feeds carry their
    // length in minutes; millilitre feeds have no duration to recover, so they
    // end when they were logged.
    const occurred = capNow(setT(Date.now(), when));
    const minutes = withAmount && this.unit === "min" ? this.amount : 0;
    const ended = Math.min(occurred + minutes * 60_000, Date.now());

    try {
      await this.store.log({
        kind: "feed",
        occurred_at: occurred,
        ended_at: ended,
        feed_kind: this.feedKind,
        amount: withAmount ? this.amount : null,
        amount_unit: withAmount ? this.unit : null,
        note: note || null,
      });
      this.close();
    } catch (error) {
      this.error(this.message(error));
    }
  }
}

defineComponent("ll-feed-sheet", LlFeedSheet);
