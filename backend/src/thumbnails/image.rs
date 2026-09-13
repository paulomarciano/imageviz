//! Image thumbnail generation using the `image` crate.
//!
//! Provides a public async function that reads a source image (PNG, JPG, WEBP, GIF),
//! downscales it to a target width with the single-pass integer sampler
//! ([`image::DynamicImage::thumbnail`]) — avoiding the intermediate full-height
//! buffer a two-pass Lanczos `resize` allocates — and encodes the result as WebP
//! at a caller-provided output path. The content-addressed cache layer
//! ([`super::cache`]) owns key computation and the atomic `{key}.tmp` → rename
//! write; nothing here touches the OS temp directory.
//!
//! # Memory behaviour (review R10)
//!
//! A header-only dimension probe ([`ImageReader::into_dimensions`]) runs before
//! decode, so absurdly large sources are rejected without allocating (see
//! [`MAX_DECODE_SIDE_PX`]). The decode + downscale happens in a scoped block, so
//! the full-size source buffer is released before WebP encoding; peak memory is
//! one decoded source plus the small output.
//!
//! # Quality note
//!
//! The single-pass sampler trades a sliver of edge sharpness for the memory
//! win: measured output differs from the previous Lanczos3 path by ≈ 0.4/255
//! mean per-channel on a 24-megapixel fixture, with dimensions identical.
//! Golden-file equivalence tests pin this in [`super::image::tests`].
//!
//! Note: like the previous `image::open` path, EXIF orientation is *not*
//! applied to the decoded pixels.

use std::path::{Path, PathBuf};

use image::ImageReader;

use super::ThumbnailError;

/// Default thumbnail width in pixels.
pub const DEFAULT_TARGET_WIDTH: u32 = 200;

/// Minimum allowed target width.
pub const MIN_WIDTH: u32 = 100;

/// Maximum allowed target width.
pub const MAX_WIDTH: u32 = 500;

/// Largest source side (width or height, in pixels) that will be decoded.
///
/// 8192 px is ≈ 268 MB as RGBA at the worst case (square) — the cap keeps the
/// thumbnail path away from unbounded decode allocations. Larger sources are
/// rejected with [`ThumbnailError::SourceTooLarge`] at the header probe,
/// before any pixel data is read.
pub const MAX_DECODE_SIDE_PX: u32 = 8192;

/// Generate a WebP thumbnail for the given source image and write it to `output_path`.
///
/// The source image is downscaled to `target_width` pixels wide while maintaining
/// the original aspect ratio using the single-pass integer sampler for efficient
/// memory use. The caller chooses the output location: the cache layer passes a
/// temporary path inside the cache directory and renames the result atomically
/// into place.
///
/// CPU-bound image processing is offloaded to `tokio::task::spawn_blocking` to avoid
/// starving the async runtime.
///
/// # Errors
/// - `InvalidWidth` if `target_width` is outside `[MIN_WIDTH, MAX_WIDTH]`
/// - `SourceNotFound` if the source file does not exist
/// - `SourceTooLarge` if the source exceeds `MAX_DECODE_SIDE_PX` on either side
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
/// Probes the source dimensions from the format header (rejecting oversized
/// sources before any decode allocation), downscales with the single-pass
/// integer sampler, and writes the WebP result to `output_path`.
fn generate_thumbnail_sync(
    source_path: &Path,
    target_width: u32,
    output_path: &Path,
) -> Result<PathBuf, ThumbnailError> {
    // Header-only probe: bounds memory before any pixel data is read.
    let (src_width, src_height) = read_dimensions(source_path)?;
    if src_width.max(src_height) > MAX_DECODE_SIDE_PX {
        return Err(ThumbnailError::SourceTooLarge {
            width: src_width,
            height: src_height,
            max_side: MAX_DECODE_SIDE_PX,
        });
    }

    // Scoped decode + downscale: the full-size source buffer is dropped here,
    // before the WebP encoder allocates.
    let thumb = {
        let img = decode_image(source_path)?;
        // `thumbnail` preserves aspect ratio and fits the image within
        // (target_width, u32::MAX) — the same dimension contract as the
        // previous `resize` call (both use `resize_dimensions` internally).
        img.thumbnail(target_width, u32::MAX)
    };

    thumb
        .save_with_format(output_path, image::ImageFormat::WebP)
        .map_err(|e| ThumbnailError::Encode(e.to_string()))?;

    Ok(output_path.to_path_buf())
}

/// Read image dimensions from the format header without decoding pixel data.
fn read_dimensions(path: &Path) -> Result<(u32, u32), ThumbnailError> {
    ImageReader::open(path)?
        .with_guessed_format()
        .map_err(ThumbnailError::Io)?
        .into_dimensions()
        .map_err(ThumbnailError::Image)
}

/// Decode the full image, auto-detecting the format from its content.
fn decode_image(path: &Path) -> Result<image::DynamicImage, ThumbnailError> {
    ImageReader::open(path)?
        .with_guessed_format()
        .map_err(ThumbnailError::Io)?
        .decode()
        .map_err(ThumbnailError::Image)
}

#[cfg(test)]
#[path = "image_test.rs"]
mod tests;
