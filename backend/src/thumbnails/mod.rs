//! Thumbnail generation for image and video media files.
//!
//! This module provides on-demand thumbnail generation using the `image` crate
//! (images) and `ffmpeg` subprocess (videos). Generated thumbnails are WebP format
//! and cached to disk using a content-addressed scheme.
//!
//! # Sub-modules
//! - `image` — Image thumbnail resampling via Lanczos3 + WebP encoding
//! - `video` — Video keyframe extraction via ffmpeg sidecar (task 2.2)
//! - `cache` — Content-addressed on-disk cache (task 2.3)

pub mod cache;
pub mod image;
pub mod video;

pub use self::cache::get_or_generate_thumbnail;
pub use self::image::generate_image_thumbnail;

use std::path::PathBuf;

/// Errors that can occur during thumbnail generation.
#[derive(Debug)]
pub enum ThumbnailError {
    /// Wraps standard I/O errors (file not found, permission denied, etc.)
    Io(std::io::Error),
    /// Wraps errors from the `image` crate (decode, encode, format issues)
    Image(::image::ImageError),
    /// Requested width is outside the valid range
    InvalidWidth {
        /// The requested width
        width: u32,
        /// Minimum allowed width
        min: u32,
        /// Maximum allowed width
        max: u32,
    },
    /// The source file does not exist at the given path
    SourceNotFound(PathBuf),
    /// Generic encoding/writing failure
    Encode(String),
}

impl std::fmt::Display for ThumbnailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThumbnailError::Io(e) => write!(f, "IO error: {e}"),
            ThumbnailError::Image(e) => write!(f, "Image error: {e}"),
            ThumbnailError::InvalidWidth { width, min, max } => {
                write!(f, "Invalid width {width}: must be between {min} and {max}")
            }
            ThumbnailError::SourceNotFound(path) => {
                write!(f, "Source not found: {}", path.display())
            }
            ThumbnailError::Encode(msg) => write!(f, "Encoding error: {msg}"),
        }
    }
}

impl std::error::Error for ThumbnailError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ThumbnailError::Io(e) => Some(e),
            ThumbnailError::Image(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ThumbnailError {
    fn from(e: std::io::Error) -> Self {
        ThumbnailError::Io(e)
    }
}

impl From<::image::ImageError> for ThumbnailError {
    fn from(e: ::image::ImageError) -> Self {
        ThumbnailError::Image(e)
    }
}
