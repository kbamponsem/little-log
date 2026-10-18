import Compressor from "compressorjs";

/** Photos are stored at most 1280 px on the long edge (matches little-log-core). */
export const PHOTO_MAX_EDGE = 1280;
export const PHOTO_QUALITY = 0.85;

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
