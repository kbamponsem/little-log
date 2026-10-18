import { defineComponent, LLComponent } from "../../core/component";
import { $ } from "../../core/dom";
import { repo } from "../../data/store";
import { LL } from "../../core/ui-store";
import type { Entry } from "../../data/model";
import { esc } from "../../format";
import "./ll-photo-grid.css";

/** A 3-up gallery of every entry with a photo. Taps emit `ll-open-entry`. */
export class LlPhotoGrid extends LLComponent {
  private entries: Entry[] = [];
  private emptyMessage = "No photos yet.";

  setEntries(entries: Entry[], emptyMessage?: string): void {
    this.entries = entries;
    if (emptyMessage !== undefined) this.emptyMessage = emptyMessage;
    this.render();
  }

  protected template(): string {
    if (!this.entries.length) {
      return `<p class="empty" style="grid-column:1/-1">${esc(this.emptyMessage)}</p>`;
    }
    return `<div class="grid"></div>`;
  }

  protected mount(): void {
    const grid = $(".grid", this);
    if (!grid) return;

    for (const entry of this.entries) {
      const button = document.createElement("button");
      button.className = "ph";
      button.type = "button";
      button.dataset.id = entry.id;
      const label = new Date(entry.occurred_at).toLocaleDateString([], {
        month: "short",
        day: "numeric",
      });
      button.innerHTML = `<img data-eid="${esc(entry.id)}" alt="Diaper photo"><small>${esc(label)}</small>`;
      button.addEventListener("click", () => this.emit(LL.openEntry, { id: entry.id }));
      grid.append(button);
    }

    void repo.fillPhotos(this);
  }
}

defineComponent("ll-photo-grid", LlPhotoGrid);
