import { defineComponent, LLComponent } from "../../core/component";
import { $ } from "../../core/dom";
import type { Entry } from "../../data/model";
import { repo } from "../../data/store";
import { entryDetail, entryTitle, esc, hm } from "../../format";
import { ic } from "../../icons";
import "./ll-entry-row.css";

/** One timeline row. Emits `ll-open-entry` with `{ id }` when tapped. */
export class LlEntryRow extends LLComponent {
  entry: Entry | null = null;

  protected template(): string {
    const entry = this.entry;
    if (!entry) return "";
    const title = esc(entryTitle(entry));
    const detail = entryDetail(entry);
    const note = entry.note ? `<small>“${esc(entry.note)}”</small>` : "";
    const photo = entry.photo ? `<img data-eid="${esc(entry.id)}" alt="Diaper photo">` : "";
    return `
      <button class="row" type="button" data-id="${esc(entry.id)}">
        <span class="tm">${hm(entry.occurred_at)}</span>
        <span class="em">${this.icon(entry)}</span>
        <span><b>${title}</b>${detail ? `<small>${esc(detail)}</small>` : ""}${note}</span>
        ${photo}
      </button>
    `;
  }

  protected mount(): void {
    const button = $(".row", this);
    button?.addEventListener("click", () => {
      if (this.entry) this.emit("ll-open-entry", { id: this.entry.id });
    });
    if (this.entry?.photo) void repo.fillPhotos(this);
  }

  private icon(entry: Entry): string {
    if (entry.kind === "feed") return ic("milk", 26);
    if (entry.kind === "sleep") return ic("moon", 26);
    if (entry.kind === "mood") return ic("heart", 26);
    return entry.diaper_type === "pee" ? ic("droplet", 26) : ic("poop", 26);
  }
}

defineComponent("ll-entry-row", LlEntryRow);
