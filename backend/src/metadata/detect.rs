use crate::metadata::png::{Metadata, parse_png_metadata};
use crate::metadata::video::parse_video_metadata;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct MediaInfo {
    pub mime_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub file_size: u64,
}

/// Detect media information from a file path.
///
/// Returns MIME type, dimensions (if available), and file size.
/// For images, dimensions are read via the `image` crate (header-only, fast).
/// For videos, dimensions are read via ffprobe (async).
pub async fn detect_media(path: &Path) -> Result<MediaInfo, DetectionError> {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    let mime_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        other => return Err(DetectionError::UnsupportedFormat(other.to_string())),
    }
    .to_string();

    let file_size = std::fs::metadata(path).map_err(DetectionError::Io)?.len();

    let (width, height) = if mime_type.starts_with("image/") {
        // Image dimension decoding is CPU-bound — run on a blocking thread.
        let path_buf = path.to_path_buf();
        tokio::task::spawn_blocking(move || detect_image_dimensions(&path_buf))
            .await
            .map_err(|join_e| DetectionError::Io(std::io::Error::other(join_e)))?
    } else {
        // Video dimensions via ffprobe
        Ok(match parse_video_metadata(path).await {
            Ok(meta) => (Some(meta.width), Some(meta.height)),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to extract video dimensions");
                (None, None)
            }
        })
    }?;

    Ok(MediaInfo { mime_type, width, height, file_size })
}

/// Extract dimensions from an image file (header-only, fast).
fn detect_image_dimensions(path: &Path) -> Result<(Option<u32>, Option<u32>), DetectionError> {
    let reader = image::ImageReader::open(path)
        .map_err(DetectionError::Io)?
        .with_guessed_format()
        .map_err(|_| DetectionError::UnknownFormat)?;

    let dimensions = reader.into_dimensions().map_err(DetectionError::Image)?;
    Ok((Some(dimensions.0), Some(dimensions.1)))
}

/// Extract all metadata from a PNG file (both dimensions and text chunks).
pub fn extract_png_metadata(path: &Path) -> Result<Metadata, crate::metadata::png::PngParseError> {
    parse_png_metadata(path)
}

#[derive(Debug)]
pub enum DetectionError {
    UnsupportedFormat(String),
    UnknownFormat,
    Io(std::io::Error),
    Image(image::ImageError),
}

impl std::fmt::Display for DetectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectionError::UnsupportedFormat(e) => write!(f, "Unsupported format: {}", e),
            DetectionError::UnknownFormat => write!(f, "Could not determine file format"),
            DetectionError::Io(e) => write!(f, "IO error: {}", e),
            DetectionError::Image(e) => write!(f, "Image error: {}", e),
        }
    }
}

impl std::error::Error for DetectionError {}

impl From<std::io::Error> for DetectionError {
    fn from(e: std::io::Error) -> Self {
        DetectionError::Io(e)
    }
}

#[cfg(test)]
#[path = "detect_test.rs"]
mod tests;
