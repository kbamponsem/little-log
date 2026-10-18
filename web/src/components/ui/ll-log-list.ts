import { defineComponent, LLComponent } from "../../core/component";
import type { Entry } from "../../data/model";
import { esc } from "../../format";
import { LlEntryRow } from "./ll-entry-row";
import "./ll-log-list.css";

/** A `<ul class="log">` of `ll-entry-row`s with an empty state. */
export class LlLogList extends LLComponent {
  private entries: Entry[] = [];
  private emptyMessage = "Nothing logged yet.";

  setEntries(entries: Entry[], emptyMessage?: string): void {
    this.entries = entries;
    if (emptyMessage !== undefined) this.emptyMessage = emptyMessage;
    this.render();
  }

  protected template(): string {
    const body = this.entries.length
      ? ""
      : `<li class="empty">${esc(this.emptyMessage)}</li>`;
    return `<ul class="log" data-slot="list">${body}</ul>`;
  }

  protected mount(): void {
    const list = this.querySelector<HTMLUListElement>("ul.log");
    if (!list || !this.entries.length) return;
    list.replaceChildren(
      ...this.entries.map((entry) => {
        const item = document.createElement("li");
        const row = new LlEntryRow();
        row.entry = entry;
        item.append(row);
        return item;
      }),
    );
  }
}

defineComponent("ll-log-list", LlLogList);
