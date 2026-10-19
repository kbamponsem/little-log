import { defineComponent, LLComponent } from "../../core/component";
import { ui } from "../../core/ui-store";
import { ic } from "../../icons";
import { store } from "../../data/store";
import "./ll-welcome.css";

/**
 * First run: ask who the log is for.
 *
 * Shown whenever the log holds no children at all — not "once per device", so
 * a phone that syncs into an existing log goes straight to Today instead of
 * being asked to add a baby that is already there.
 *
 * A child is added as soon as its name is entered, so "or babies" is just
 * pressing "Add another"; each one gets its own id, its own group key and its
 * own first profile event on the way in (see `create_baby`).
 *
 * The other half of the question is "or am I joining someone else's log?" —
 * which is what a second parent is doing, on a phone with nothing on it yet.
 * That path lives behind *Join with a code*, and once the child arrives by
 * sync this screen has nothing left to ask, so it goes away.
 */
export class LlWelcome extends LLComponent {
  private added: string[] = [];
  private busy = false;
  private error = "";

  protected template(): string {
    return `
      <div class="welcome">
        <div class="welcome-head">
          <div class="mark" aria-hidden="true">${ic("moon", 44)}</div>
          <button class="cog" type="button" data-action="theme" aria-label="Appearance">
            ${ic("settings", 28)}
          </button>
        </div>
        <h1>Who are we logging?</h1>
        <p class="sub">
          Add your baby — or babies. Everything you log is grouped around a
          child, and each one keeps its own feeds, sleeps and diapers.
        </p>
        <p class="note">
          Everything you log is stored on this device. Nothing is sent anywhere,
          and there is no account to create.
        </p>

        <ll-card class="card">
          <ll-field name="name" label="Name" flush autocomplete="off"></ll-field>
          <ll-field name="born" label="Birthday" type="date"></ll-field>
        </ll-card>

        <p class="error" data-slot="error" hidden></p>

        <button class="chip primary" type="button" data-action="add" disabled>
          ${ic("plus", 18)}<span data-slot="add-label">Add</span>
        </button>

        <div class="added" data-slot="added" hidden>
          <div class="grp">Added</div>
          <ul data-slot="added-list"></ul>
        </div>

        <button class="chip start" type="button" data-action="start" hidden>
          ${ic("check", 18)} Start logging
        </button>

        <div class="or"><span>or</span></div>

        <button class="chip join" type="button" data-action="join">
          ${ic("baby", 18)} Join with a code
        </button>
        <p class="note">
          Already logging on another phone — your partner's, say? Join that log
          instead: scan their invite code, or paste the link.
        </p>
      </div>
    `;
  }

  protected mount(): void {
    this.syncForm();
  }

  protected subscribe(): void {
    this.addEventListener("input", () => this.syncForm());
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "add") void this.add();
      if (action === "start") this.finish();
      if (action === "theme") ui.openSheet({ name: "theme" });
      if (action === "join") ui.openSheet({ name: "join" });
    });
    this.addEventListener("keydown", (event) => {
      if ((event as KeyboardEvent).key === "Enter") void this.add();
    });
  }

  private nameField(): HTMLInputElement | null {
    return this.querySelector<HTMLInputElement>("ll-field[name='name'] input");
  }

  private bornField(): HTMLInputElement | null {
    return this.querySelector<HTMLInputElement>("ll-field[name='born'] input");
  }

  private syncForm(): void {
    const name = this.nameField()?.value.trim() ?? "";

    const add = this.querySelector<HTMLButtonElement>("[data-action='add']");
    if (add) add.disabled = this.busy || name.length === 0;

    const label = this.querySelector<HTMLElement>('[data-slot="add-label"]');
    if (label) label.textContent = this.added.length ? "Add another" : "Add";

    const added = this.querySelector<HTMLElement>('[data-slot="added"]');
    if (added) added.hidden = this.added.length === 0;

    const list = this.querySelector<HTMLElement>('[data-slot="added-list"]');
    if (list) {
      list.replaceChildren(
        ...this.added.map((entry) => {
          const item = document.createElement("li");
          item.textContent = entry;
          return item;
        }),
      );
    }

    const start = this.querySelector<HTMLButtonElement>("[data-action='start']");
    if (start) start.hidden = this.added.length === 0;

    const slot = this.querySelector<HTMLElement>('[data-slot="error"]');
    if (slot) {
      slot.hidden = !this.error;
      slot.textContent = this.error;
    }
  }

  private async add(): Promise<void> {
    if (this.busy) return;
    const name = this.nameField()?.value.trim() ?? "";
    if (!name) return;

    this.busy = true;
    this.error = "";
    this.syncForm();
    try {
      const born = this.bornField()?.value ?? "";
      await store.addBaby(name, born);
      this.added = [...this.added, born ? `${name} · born ${born}` : name];
      this.clearFields();
    } catch (error) {
      this.error = `Could not add ${name}: ${error instanceof Error ? error.message : error}`;
    } finally {
      this.busy = false;
      this.syncForm();
    }
  }

  private clearFields(): void {
    const name = this.nameField();
    if (name) name.value = "";
    const born = this.bornField();
    if (born) born.value = "";
    name?.focus();
  }

  /** The log now has a child, so the app can open onto Today. */
  private finish(): void {
    if (this.added.length === 0) return;
    this.dispatchEvent(new CustomEvent("ll-welcome-done", { bubbles: true }));
  }
}

defineComponent("ll-welcome", LlWelcome);
