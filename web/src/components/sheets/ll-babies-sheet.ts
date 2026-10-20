import { defineComponent } from "../../core/component";
import { LLSheet } from "../../core/sheet";
import { ui } from "../../core/ui-store";
import { ageStr, esc } from "../../format";
import type { Baby } from "../../data/model";
import "./ll-babies-sheet.css";

/**
 * Switch which child the log is showing, and add another.
 *
 * Reached by holding the Baby tab instead of listing every child on the home
 * screen: a household with one child never sees a switcher, and a household
 * with three does not lose its header to a row of chips. The choice is local to
 * this device (see `AppStore.selectBaby`), so it does not travel anywhere.
 */
export class LlBabiesSheet extends LLSheet {
  /** Keyboard users land on the child they can switch to. */
  autofocusSelector = "[data-baby]";

  protected template(): string {
    const current = this.store.baby;
    const babies = this.store.babies;
    const rows = babies.length
      ? babies.map((baby) => row(baby, baby.id === current.id)).join("")
      : `<p class="note">No children yet.</p>`;

    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>Who are you logging for?</h3>
      <div class="babies">${rows}</div>
      <button class="add-baby" type="button" data-action="add">+ Add baby</button>
    `;
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      const target = event.target as HTMLElement;
      if (target.closest("[data-action='close']")) {
        this.close();
        return;
      }
      if (target.closest("[data-action='add']")) {
        ui.openSheet({ name: "add-baby" });
        return;
      }
      const chosen = target.closest<HTMLElement>("[data-baby]")?.dataset.baby;
      if (chosen) void this.pick(chosen);
    });
  }

  private async pick(babyId: string): Promise<void> {
    try {
      await this.store.selectBaby(babyId);
      this.close();
    } catch (error) {
      this.error(this.message(error));
    }
  }
}

/** One child, with the one being shown marked. */
function row(baby: Baby, current: boolean): string {
  return `
    <button class="baby" type="button" data-baby="${esc(baby.id ?? "")}"
            ${current ? 'aria-current="true"' : ""}>
      <span class="baby-name">${esc(baby.name || "Baby")}</span>
      <span class="baby-age">${baby.born ? esc(ageStr(baby.born)) : "No birthday yet"}</span>
    </button>
  `;
}

defineComponent("ll-babies-sheet", LlBabiesSheet);
