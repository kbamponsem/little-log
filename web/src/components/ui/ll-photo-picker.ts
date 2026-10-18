import { defineComponent, LLComponent } from "../../core/component";
import { $ } from "../../core/dom";
import { resizePhoto } from "../../photo";
import { ic } from "../../icons";
import "./ll-photo-picker.css";

/**
 * "Add photo" button with a live preview.
 *
 * Emits `ll-photo-picked` with `{ blob }` once the picked image has been
 * downscaled to ≤1280 px JPEG. `blob` is the current selection (or null).
 */
export class LlPhotoPicker extends LLComponent {
  blob: Blob | null = null;

  protected template(): string {
    return `
      <button class="chip" type="button" data-action="pick">${ic("camera", 22)} Add photo</button>
      <input type="file" accept="image/*" capture="environment" hidden>
      <img alt="Photo preview">
      <p class="err" hidden></p>
    `;
  }

  protected mount(): void {
    const button = $<HTMLButtonElement>("[data-action='pick']", this);
    const input = $<HTMLInputElement>("input[type=file]", this);
    button?.addEventListener("click", () => input?.click());
    input?.addEventListener("change", () => this.picked(input));

    if (this.blob) this.showPreview(this.blob);
  }

  private async picked(input: HTMLInputElement): Promise<void> {
    const file = input.files?.[0];
    if (!file) return;
    this.setError(null);
    try {
      const blob = await resizePhoto(file);
      this.blob = blob;
      this.showPreview(blob);
      this.emit("ll-photo-picked", { blob });
    } catch (error) {
      this.setError(error instanceof Error ? error.message : "Could not read photo");
    }
  }

  private showPreview(blob: Blob): void {
    const image = $<HTMLImageElement>("img", this);
    const button = $<HTMLButtonElement>("[data-action='pick']", this);
    if (image) {
      image.src = URL.createObjectURL(blob);
      image.style.display = "block";
    }
    if (button) button.textContent = "Photo added ✓";
  }

  private setError(message: string | null): void {
    const node = this.querySelector<HTMLElement>(".err");
    if (!node) return;
    node.textContent = message ?? "";
    node.hidden = !message;
  }
}

defineComponent("ll-photo-picker", LlPhotoPicker);
