import { LLSheet } from "../../core/sheet";
import type { LlChipGroup } from "../ui/ll-chip-group";
import type { LlStepper } from "../ui/ll-stepper";
import { defineComponent } from "../../core/component";
import { capNow, setT, tv } from "../../format";
import { FEED_KINDS } from "../../data/model";
import type { AmountUnit, FeedKind } from "../../data/model";

/** Log a feed. Breast left is selected by default, matching the prototype. */
export class LlFeedSheet extends LLSheet {
  private feedKind: FeedKind = "breast_left";
  private amount = 10;
  private unit: AmountUnit = "min";
  private stepper: LlStepper | null = null;
  private label: HTMLElement | null = null;

  protected template(): string {
    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>Feeding</h3>
      <div class="grp">What kind?</div>
      <ll-chip-group data-role="kinds"></ll-chip-group>
      <div class="grp" data-slot="amount-label">Minutes</div>
      <ll-stepper data-role="stepper"></ll-stepper>
      <ll-field name="when" label="When" type="time" value="${tv(Date.now())}"></ll-field>
      <ll-field name="note" label="Notes" multiline placeholder="Anything worth remembering?"></ll-field>
      <p class="err" hidden></p>
      <button class="go" type="button" data-action="save">Log feeding now</button>
    `;
  }

  protected mount(): void {
    this.stepper = this.querySelector<LlStepper>('[data-role="stepper"]');
    this.label = this.querySelector<HTMLElement>('[data-slot="amount-label"]');

    this.querySelector<LlChipGroup>('[data-role="kinds"]')?.configure({
      options: FEED_KINDS,
      value: this.feedKind,
      variant: "card",
      // Feed kinds are a required single choice — no tap-to-clear.
      toggle: false,
    });

    this.stepper?.addEventListener("ll-step", (event) => {
      this.amount = (event as CustomEvent<{ value: number }>).detail.value;
    });

    this.applyKind();
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "close") this.close();
      if (action === "save") void this.save();
    });

    this.addEventListener("ll-change", (event) => {
      this.feedKind = (event as CustomEvent<{ value: FeedKind }>).detail.value;
      this.applyKind();
    });
  }

  /** Show/hide the stepper and keep its unit in sync with the feed kind. */
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
      this.label.textContent = bottle ? "Amount" : "Minutes";
    }
    if (this.stepper) {
      this.stepper.hidden = !withAmount;
      this.stepper.configure(this.amount, this.unit);
    }
  }

  private async save(): Promise<void> {
    this.error(null);
    const when = this.field("when") || tv(Date.now());
    const note = this.field("note");
    const bottle = this.feedKind === "bottle";
    const breast = this.feedKind === "breast_left" || this.feedKind === "breast_right";
    const withAmount = bottle || breast;

    try {
      await this.store.log({
        kind: "feed",
        occurred_at: capNow(setT(Date.now(), when)),
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
