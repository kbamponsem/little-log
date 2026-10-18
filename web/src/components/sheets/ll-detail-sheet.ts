import { defineComponent } from "../../core/component";
import { LLSheet } from "../../core/sheet";
import type { Entry } from "../../data/model";
import { entryDetail, entryTitle, esc, setT, tv } from "../../format";
import { repo } from "../../data/store";

/** Edit or delete an existing entry. */
export class LlDetailSheet extends LLSheet {
  entryId = "";

  private entry(): Entry | undefined {
    return this.store.find(this.entryId);
  }

  protected template(): string {
    const entry = this.entry();
    if (!entry) return `<button class="x" type="button" data-action="close">×</button>`;
    const sleep = entry.kind === "sleep";
    const kindTitle = sleep ? "Nap" : entry.kind === "feed" ? "Feeding" : "Diaper change";
    const photo = entry.photo ? `<img class="bigph" data-eid="${esc(entry.id)}" alt="Diaper photo">` : "";
    const times = sleep
      ? `
        <div class="two">
          <ll-field name="from" label="Fell asleep" type="time" value="${tv(entry.occurred_at)}"></ll-field>
          <ll-field name="to" label="Woke up" type="time" value="${tv(entry.ended_at ?? Date.now())}"></ll-field>
        </div>
      `
      : `<ll-field name="time" label="Time" type="time" value="${tv(entry.occurred_at)}"></ll-field>`;

    return `
      <button class="x" type="button" data-action="close" aria-label="Close">×</button>
      <h3>${kindTitle}</h3>
      ${photo}
      <p><b>${esc(entryTitle(entry))}</b><br>${esc(entryDetail(entry))}</p>
      ${times}
      <ll-field name="note" label="Notes" multiline value="${esc(entry.note ?? "")}"></ll-field>
      <p class="err" hidden></p>
      <button class="go" type="button" data-action="save">Save changes</button>
      <button class="del" type="button" data-action="delete">Delete entry</button>
    `;
  }

  protected mount(): void {
    this.autofocusSelector = "input[type=time]";
    if (this.entry()?.photo) void repo.fillPhotos(this);
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "close") this.close();
      if (action === "save") void this.save();
      if (action === "delete") void this.removeEntry();
    });
  }

  private async save(): Promise<void> {
    const entry = this.entry();
    if (!entry) return this.close();
    this.error(null);

    const note = this.field("note");
    const updated: Entry = { ...entry, note: note || null, updated_at: Date.now() };

    if (entry.kind === "sleep") {
      const started = setT(entry.occurred_at, this.field("from") || tv(entry.occurred_at));
      let ended = setT(entry.ended_at ?? Date.now(), this.field("to") || tv(Date.now()));
      if (ended <= started) ended += 24 * 60 * 60 * 1000;
      updated.occurred_at = started;
      updated.ended_at = ended;
    } else {
      updated.occurred_at = setT(entry.occurred_at, this.field("time") || tv(entry.occurred_at));
    }

    try {
      await this.store.saveEntry(updated);
      this.close();
    } catch (error) {
      this.error(this.message(error));
    }
  }

  private async removeEntry(): Promise<void> {
    try {
      await this.store.deleteEntry(this.entryId);
      this.close();
    } catch (error) {
      this.error(this.message(error));
    }
  }
}

defineComponent("ll-detail-sheet", LlDetailSheet);
