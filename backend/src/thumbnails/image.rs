//! Image thumbnail generation using the `image` crate.
//!
//! Provides a public async function that reads a source image (PNG, JPG, WEBP, GIF),
//! resizes it to a target width using Lanczos3 filtering, and encodes the result as
//! WebP at a caller-provided output path. The content-addressed cache layer
//! ([`super::cache`]) owns key computation and the atomic `{key}.tmp` → rename write;
//! nothing here touches the OS temp directory.

use std::path::{Path, PathBuf};

use super::ThumbnailError;

/// Default thumbnail width in pixels.
pub const DEFAULT_TARGET_WIDTH: u32 = 200;

/// Minimum allowed target width.
pub const MIN_WIDTH: u32 = 100;

/// Maximum allowed target width.
pub const MAX_WIDTH: u32 = 500;

/// Generate a WebP thumbnail for the given source image and write it to `output_path`.
///
/// The source image is resized to `target_width` pixels wide while maintaining the
/// original aspect ratio using Lanczos3 filtering for high-quality downscaling.
/// The caller chooses the output location: the cache layer passes a temporary path
/// inside the cache directory and renames the result atomically into place.
///
/// CPU-bound image processing is offloaded to `tokio::task::spawn_blocking` to avoid
/// starving the async runtime.
///
/// # Errors
/// - `InvalidWidth` if `target_width` is outside `[MIN_WIDTH, MAX_WIDTH]`
/// - `SourceNotFound` if the source file does not exist
/// - `Image` if the source cannot be decoded
/// - `Encode` if the WebP output cannot be written (or the blocking task panicked)
pub async fn generate_image_thumbnail(
    source_path: &Path,
    target_width: u32,
    output_path: &Path,
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

    let source = source_path.to_path_buf();
    let output = output_path.to_path_buf();

    // Offload CPU-bound image processing to blocking thread pool
    tokio::task::spawn_blocking(move || generate_thumbnail_sync(&source, target_width, &output))
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

#[cfg(test)]
#[path = "image_test.rs"]
mod tests;
