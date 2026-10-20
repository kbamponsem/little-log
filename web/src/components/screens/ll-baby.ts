import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { defineComponent, LLComponent } from "../../core/component";
import { ui } from "../../core/ui-store";
import type { Baby } from "../../data/model";
import { store } from "../../data/store";
import { ageStr, esc } from "../../format";
import { ic } from "../../icons";
import { choosePhotoSource, resizePhoto } from "../../photo";
import "./ll-baby.css";

/**
 * The current child's profile: who you are logging for, their details and this
 * device's copy of the log.
 *
 * Everything about the household — switching or adding a child, caregivers,
 * invites, joining — lives elsewhere: the child switcher opens by holding the
 * Baby tab, and sharing is device-level, so it lives in Settings.
 */
export class LlBaby extends LLComponent {
  private saveTimer = 0;
  private deleteArmed = false;

  protected template(): string {
    const baby = store.baby;
    const name = baby.name || "Baby";
    const age = baby.born ? ageStr(baby.born) : "";
    const sub = !baby.born
      ? "No birthday yet"
      : age === "Not yet born"
        ? age
        : `${age} · born ${bornStr(baby.born)}`;

    return `
      <header class="baby-head">
        <div class="baby-avatar-wrap" data-action="pick-photo">
          <button class="baby-avatar" type="button" data-action="pick-photo" aria-label="Change profile photo">
            <img data-role="baby-photo" alt="" hidden>
            <span class="baby-initial" data-role="baby-initial">${esc(initialOf(name))}</span>
          </button>
          <span class="baby-camera" aria-hidden="true">${ic("camera", 14)}</span>
        </div>
        <input class="photo-input" type="file" accept="image/*" data-source="upload" hidden>
        <input class="photo-input" type="file" accept="image/*" data-source="camera" capture="environment" hidden>
        <div class="baby-who">
          <h1>${esc(name)}</h1>
          <p class="sub">${esc(sub)}</p>
        </div>
        <button class="cog" type="button" data-action="settings" aria-label="Settings">
          ${ic("settings", 28)}
        </button>
      </header>

      <ll-card class="card" heading="Profile">
        <ll-field name="name" label="Name" flush value="${esc(baby.name)}"></ll-field>
        <ll-field name="born" label="Birthday" type="date" value="${esc(baby.born)}"></ll-field>
      </ll-card>

      <ll-card class="card" heading="Your data">
        <div class="chips">
          <button class="chip" type="button" data-action="export">Export to file</button>
          <button class="chip delete-button" type="button" data-action="delete-all">Delete everything</button>
        </div>
        <p class="note">Everything is stored on this device. Nothing is sent anywhere.</p>
      </ll-card>
    `;
  }

  protected mount(): void {
    void this.fillPhoto();
  }

  protected subscribe(): void {
    this.watch(store);

    this.addEventListener("change", (event) => {
      const input = event.target as HTMLInputElement;
      if (input.classList.contains("photo-input")) void this.pickPhoto(input);
    });

    this.addEventListener("input", (event) => {
      const control = event.target as HTMLInputElement;
      if (control.name === "name") this.saveSoon({ name: control.value });
      if (control.name === "born") this.saveSoon({ born: control.value });
    });

    this.addEventListener("click", (event) => {
      const target = event.target as HTMLElement;
      const action = target.closest<HTMLElement>("[data-action]")?.dataset.action;
      if (action === "settings") ui.openOverlay("settings");
      if (action === "pick-photo") {
        choosePhotoSource((source) =>
          this.querySelector<HTMLInputElement>(`.photo-input[data-source='${source}']`)?.click(),
        );
      }
      if (action === "export") void this.export(event);
      if (action === "delete-all") void this.deleteAll(event);
    });
  }

  /** Show the current child's picture, when this device has one. */
  private async fillPhoto(): Promise<void> {
    const id = store.baby.id;
    const photo = store.baby.photo;
    if (!id || !photo) return;
    try {
      const src = await store.repo.babyPhotoDataUrl(id);
      // The screen may have been re-rendered or switched child while reading.
      if (store.baby.id !== id || store.baby.photo !== photo) return;
      const image = this.querySelector<HTMLImageElement>('[data-role="baby-photo"]');
      const initial = this.querySelector<HTMLElement>('[data-role="baby-initial"]');
      if (image) {
        image.src = src;
        image.hidden = false;
      }
      if (initial) initial.hidden = true;
    } catch {
      // No file on this device yet: keep the initial.
    }
  }

  private async pickPhoto(input: HTMLInputElement): Promise<void> {
    const file = input.files?.[0];
    if (!file) return;
    try {
      const blob = await resizePhoto(file);
      // Show the choice at once, then let the save re-render from disk.
      const image = this.querySelector<HTMLImageElement>('[data-role="baby-photo"]');
      const initial = this.querySelector<HTMLElement>('[data-role="baby-initial"]');
      const preview = URL.createObjectURL(blob);
      if (image) {
        image.src = preview;
        image.hidden = false;
      }
      if (initial) initial.hidden = true;
      try {
        await store.saveBabyPhoto(blob);
      } finally {
        URL.revokeObjectURL(preview);
      }
    } catch (error) {
      ui.toast("error", error instanceof Error ? error.message : String(error));
      void this.fillPhoto();
    } finally {
      input.value = "";
    }
  }

  private saveSoon(patch: Partial<Baby>): void {
    window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => {
      void store.saveBaby(patch).catch(() => undefined);
    }, 400);
  }

  private async export(event: MouseEvent): Promise<void> {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-action='export']");
    if (!button) return;
    let csv = "";
    try {
      csv = await store.repo.csv();
    } catch (error) {
      window.alert(`Export failed: ${error instanceof Error ? error.message : error}`);
      return;
    }
    try {
      const path = await save({
        title: "Export Little Log",
        defaultPath: "little-log.csv",
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!path) return;
      await writeTextFile(path, csv);
      button.textContent = "Exported ✓";
    } catch (error) {
      window.alert(`Export failed: ${error instanceof Error ? error.message : error}`);
    }
  }

  private async deleteAll(event: MouseEvent): Promise<void> {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-action='delete-all']");
    if (!button) return;
    if (!this.deleteArmed) {
      this.deleteArmed = true;
      button.textContent = "Tap again to confirm";
      return;
    }
    this.deleteArmed = false;
    try {
      await store.deleteAll();
      button.textContent = "Delete everything";
    } catch {
      button.textContent = "Failed — try again";
    }
  }
}

/** The letter in the avatar: the child's initial, or the first of "Baby". */
function initialOf(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "•";
}

/** "19 Oct 2026" for the header line, read as a local date. */
function bornStr(born: string): string {
  const [year, month, day] = born.split("-").map(Number);
  const date = year && month && day ? new Date(year, month - 1, day) : new Date(born);
  return date.toLocaleDateString([], {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

defineComponent("ll-baby", LlBaby);
