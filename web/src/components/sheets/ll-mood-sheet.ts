import { defineComponent } from "../../core/component";
import { LLSheet } from "../../core/sheet";
import { MOODS } from "../../data/model";
import type { Mood } from "../../data/model";
import { capNow, setT, tv } from "../../format";
import type { LlChipGroup } from "../ui/ll-chip-group";

/** Log how the baby felt overall during the day. */
export class LlMoodSheet extends LLSheet {
  private mood: Mood | null = null;

  protected template(): string {
    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>How was the day?</h3>
      <p class="note">How has baby felt overall?</p>
      <div class="grp">Mood</div>
      <ll-chip-group data-role="mood"></ll-chip-group>
      <ll-field name="when" label="When" type="time" value="${tv(Date.now())}"></ll-field>
      <ll-field name="note" label="Notes" multiline placeholder="Anything worth remembering?"></ll-field>
      <p class="err" hidden></p>
      <button class="go" type="button" data-action="save">Save mood</button>
    `;
  }

  protected mount(): void {
    this.querySelector<LlChipGroup>('[data-role="mood"]')?.configure({
      options: MOODS,
      value: null,
      variant: "mood",
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
      if ((event.target as HTMLElement).dataset.role === "mood") {
        this.mood = (event as CustomEvent<{ value: string | null }>).detail.value as Mood | null;
      }
    });
  }

  private async save(): Promise<void> {
    this.error(null);
    if (!this.mood) {
      this.error("Pick how baby felt.");
      return;
    }

    const when = this.field("when") || tv(Date.now());
    const note = this.field("note");

    try {
      await this.store.log({
        kind: "mood",
        occurred_at: capNow(setT(Date.now(), when)),
        mood: this.mood,
        note: note || null,
      });
      this.close();
    } catch (error) {
      this.error(this.message(error));
    }
  }
}

defineComponent("ll-mood-sheet", LlMoodSheet);
