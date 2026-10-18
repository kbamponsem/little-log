//! Photo handling: downscale to ≤1280 px and re-encode as JPEG.
//!
//! Re-encoding also strips EXIF (including GPS), so what we store on disk or
//! ship to the sync server carries no location metadata.

use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};
use crate::validate::now_millis;

/// Longest edge kept for stored photos.
pub const MAX_EDGE: u32 = 1280;
/// JPEG quality for re-encoded photos.
pub const JPEG_QUALITY: u8 = 85;

/// Decode any supported image, downscale to [`MAX_EDGE`] and re-encode as JPEG.
/// Images already small enough are re-encoded but never upscaled.
pub fn process_photo(bytes: &[u8]) -> Result<Vec<u8>> {
    let image = image::load_from_memory(bytes)?;
    let resized = if image.width().max(image.height()) > MAX_EDGE {
        image.resize(MAX_EDGE, MAX_EDGE, image::imageops::FilterType::Lanczos3)
    } else {
        image
    };

    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
    encoder.encode_image(&resized)?;
    Ok(out)
}

/// `<photos>/<entry id>.jpg`
pub fn photo_path(dir: &Path, entry_id: &str) -> PathBuf {
    dir.join(format!("{entry_id}.jpg"))
}

/// Process and store a photo for an entry; returns the file name to record on
/// the entry (always `"<entry id>.jpg"`).
pub fn write_photo(dir: &Path, entry_id: &str, bytes: &[u8]) -> Result<String> {
    let processed = process_photo(bytes)?;
    let path = photo_path(dir, entry_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CoreError::io(parent.display().to_string(), error))?;
    }
    write_atomic(&path, &processed)?;
    Ok(format!("{entry_id}.jpg"))
}

pub fn read_photo(dir: &Path, entry_id: &str) -> Result<Vec<u8>> {
    let path = photo_path(dir, entry_id);
    std::fs::read(&path).map_err(|error| CoreError::io(path.display().to_string(), error))
}

pub fn remove_photo(dir: &Path, entry_id: &str) -> Result<()> {
    let path = photo_path(dir, entry_id);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CoreError::io(path.display().to_string(), error)),
    }
}

/// Write to a temporary file, then rename: a crash never leaves a half photo.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension(format!("tmp{}", now_millis()));
    std::fs::write(&temp, bytes)
        .map_err(|error| CoreError::io(temp.display().to_string(), error))?;
    std::fs::rename(&temp, path).map_err(|error| CoreError::io(path.display().to_string(), error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn big_photos_are_downscaled_to_the_max_edge() {
        let out = process_photo(&png(3000, 2000)).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert!(decoded.width().max(decoded.height()) <= MAX_EDGE);
        assert_eq!(decoded.width(), MAX_EDGE);
        // JPEG magic bytes.
        assert_eq!(&out[..2], &[0xff, 0xd8]);
    }

    #[test]
    fn small_photos_are_never_upscaled() {
        let out = process_photo(&png(320, 200)).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (320, 200));
    }

    #[test]
    fn photos_round_trip_through_the_photos_directory() {
        let dir = std::env::temp_dir().join(format!("little-log-photos-{}", now_millis()));
        let id = uuid::Uuid::now_v7().to_string();
        let name = write_photo(&dir, &id, &png(2000, 1000)).unwrap();
        assert_eq!(name, format!("{id}.jpg"));

        let bytes = read_photo(&dir, &id).unwrap();
        assert_eq!(&bytes[..2], &[0xff, 0xd8]);

        remove_photo(&dir, &id).unwrap();
        assert!(read_photo(&dir, &id).is_err());
        // Removing twice is fine.
        remove_photo(&dir, &id).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn garbage_input_is_rejected() {
        assert!(process_photo(b"not an image").is_err());
    }
}
