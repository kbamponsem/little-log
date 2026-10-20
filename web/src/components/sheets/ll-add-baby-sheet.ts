import { defineComponent } from "../../core/component";
import { LLSheet } from "../../core/sheet";
import { ui } from "../../core/ui-store";

/**
 * Add another child to the household, or join someone else's log.
 *
 * Opened from the child switcher (hold the Baby tab), which is where children
 * are both switched and added, so the Baby screen itself stays a profile page.
 * Adding makes the new child the one being shown (see `AppStore.addBaby`).
 */
export class LlAddBabySheet extends LLSheet {
  autofocusSelector = "ll-field[name='name'] input";

  private busy = false;

  protected template(): string {
    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>Add a baby</h3>
      <ll-field name="name" label="Name" placeholder="Name"></ll-field>
      <ll-field name="born" label="Birthday" type="date"></ll-field>
      <p class="err" hidden></p>
      <button class="go" type="button" data-action="add">Add baby</button>
      <div class="grp">Or join someone else's</div>
      <button class="chip join-button" type="button" data-action="join">Join with a code</button>
    `;
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "close") this.close();
      if (action === "add") void this.add();
      if (action === "join") ui.openSheet({ name: "join" });
    });
  }

  private async add(): Promise<void> {
    if (this.busy) return;
    const name = this.field("name");
    if (!name) {
      this.error("Give the baby a name.");
      return;
    }

    this.busy = true;
    this.error(null);
    try {
      await this.store.addBaby(name, this.field("born"));
      ui.toast("success", `Added ${name}.`);
      this.close();
    } catch (error) {
      this.error(this.message(error));
      this.busy = false;
    }
  }
}

defineComponent("ll-add-baby-sheet", LlAddBabySheet);
