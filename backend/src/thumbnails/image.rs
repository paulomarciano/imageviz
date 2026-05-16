//! Image thumbnail generation using the `image` crate.
//!
//! Provides a public async function that reads a source image (PNG, JPG, WEBP, GIF),
//! resizes it to a target width using Lanczos3 filtering, and encodes the result as
//! WebP. The output path is deterministically derived from the source path and width
//! to enable caching without additional infrastructure.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::ThumbnailError;

/// Default thumbnail width in pixels.
pub const DEFAULT_TARGET_WIDTH: u32 = 200;

/// Minimum allowed target width.
pub const MIN_WIDTH: u32 = 100;

/// Maximum allowed target width.
pub const MAX_WIDTH: u32 = 500;

/// Generate a WebP thumbnail for the given source image.
///
/// The source image is resized to `target_width` pixels wide while maintaining the
/// original aspect ratio using Lanczos3 filtering for high-quality downscaling.
/// The output path is deterministically computed from the absolute source path and
/// target width via SHA-256, placed in the system temp directory.
///
/// CPU-bound image processing is offloaded to `tokio::task::spawn_blocking` to avoid
/// starving the async runtime.
///
/// # Errors
/// - `InvalidWidth` if `target_width` is outside `[MIN_WIDTH, MAX_WIDTH]`
/// - `SourceNotFound` if the source file does not exist
/// - `Io` if the path cannot be canonicalised
/// - `Image` if the source cannot be decoded
/// - `Encode` if the WebP output cannot be written
pub async fn generate_image_thumbnail(
    source_path: &Path,
    target_width: u32,
) -> Result<PathBuf, ThumbnailError> {
    // --- Validate width range ---
    if !(MIN_WIDTH..=MAX_WIDTH).contains(&target_width) {
        return Err(ThumbnailError::InvalidWidth {
            width: target_width,
            min: MIN_WIDTH,
            max: MAX_WIDTH,
        });
    }

    // --- Validate source exists ---
    if !source_path.exists() {
        return Err(ThumbnailError::SourceNotFound(source_path.to_path_buf()));
    }

    let source_abs = source_path
        .canonicalize()
        .map_err(ThumbnailError::Io)?;

    let output_path = thumbnail_output_path(&source_abs, target_width);

    // Return cached result if already generated
    if output_path.exists() {
        return Ok(output_path);
    }

    let source_clone = source_abs.clone();
    let output_clone = output_path.clone();

    // Offload CPU-bound image processing to blocking thread pool
    tokio::task::spawn_blocking(move || {
        generate_thumbnail_sync(&source_clone, target_width, &output_clone)
    })
    .await
    .map_err(|e| ThumbnailError::Encode(format!("Task join error: {e}")))?
}

/// Synchronous thumbnail generation — runs inside `spawn_blocking`.
///
/// Opens the source image, resizes with Lanczos3 aspect-ratio-preserving
/// scaling, and writes the WebP result to `output_path`.
fn generate_thumbnail_sync(
    source_path: &Path,
    target_width: u32,
    output_path: &Path,
) -> Result<PathBuf, ThumbnailError> {
    let img = image::open(source_path).map_err(ThumbnailError::Image)?;

    let resized = img.resize(target_width, u32::MAX, image::imageops::FilterType::Lanczos3);

    resized
        .save_with_format(output_path, image::ImageFormat::WebP)
        .map_err(|e| ThumbnailError::Encode(e.to_string()))?;

    Ok(output_path.to_path_buf())
}

/// Compute a deterministic output path based on the source path and target width.
///
/// Produces `{temp_dir}/{sha256_of_abs_path:width}.webp` so the same source + width
/// combination always yields the same path, enabling implicit caching.
fn thumbnail_output_path(source_abs: &Path, target_width: u32) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(source_abs.to_string_lossy().as_bytes());
    hasher.update(b":");
    hasher.update(target_width.to_string().as_bytes());
    let hash = hex::encode(hasher.finalize());

    std::env::temp_dir().join(format!("{hash}.webp"))
}

#[cfg(test)]
#[path = "image_test.rs"]
mod tests;
