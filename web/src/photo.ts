import Compressor from "compressorjs";
import "./photo-source.css";

/** Where a new picture comes from. */
export type PhotoSource = "camera" | "upload";

/** Photos are stored at most 1280 px on the long edge (matches little-log-core). */
export const PHOTO_MAX_EDGE = 1280;
export const PHOTO_QUALITY = 0.85;

/**
 * Ask whether a picture should be taken now or picked from storage.
 *
 * `onPick` runs inside the choice's click, so the caller's hidden file input
 * still opens from a user gesture. Tapping the backdrop or Escape cancels.
 */
export function choosePhotoSource(onPick: (source: PhotoSource) => void): void {
  const dialog = document.createElement("dialog");
  dialog.className = "photo-source";
  dialog.setAttribute("aria-label", "Add a photo");
  dialog.innerHTML = `
    <p class="grp">Add a photo</p>
    <button class="go" type="button" data-source="camera">Take photo</button>
    <button class="chip" type="button" data-source="upload">Upload file</button>
    <button class="link" type="button" data-action="cancel">Cancel</button>
  `;

  dialog.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    if (target === dialog || target.closest("[data-action='cancel']")) {
      dialog.close();
      return;
    }
    const source = target.closest<HTMLElement>("[data-source]")?.dataset.source;
    if (source !== "camera" && source !== "upload") return;
    dialog.close();
    onPick(source);
  });
  dialog.addEventListener("close", () => dialog.remove(), { once: true });

  document.body.append(dialog);
  dialog.showModal();
  dialog.querySelector<HTMLButtonElement>("button")?.focus();
}

/**
 * Downscale a picked photo to ≤1280 px and re-encode it as JPEG.
 * Never upscales; EXIF orientation is respected by Compressor.js.
 */
export function resizePhoto(file: Blob): Promise<Blob> {
  return new Promise((resolve, reject) => {
    new Compressor(file, {
      maxWidth: PHOTO_MAX_EDGE,
      maxHeight: PHOTO_MAX_EDGE,
      quality: PHOTO_QUALITY,
      mimeType: "image/jpeg",
      success: (result) => resolve(result),
      error: (error) =>
        reject(error instanceof Error ? error : new Error("Could not process photo")),
    });
  });
}
