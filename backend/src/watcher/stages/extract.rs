//! Stage 1: Extract file data from disk (async I/O, no DB lock held).
//!
//! Computes a SHA-256 hash, detects media type and dimensions, extracts PNG
//! metadata chunks (ComfyUI-style), and reads filesystem timestamps.

use crate::metadata::detect::MediaInfo;
use crate::metadata::detect::detect_media;
use crate::metadata::png::parse_png_metadata;
use crate::scanner::hasher::compute_file_hash;
use crate::watcher::handler::system_time_to_iso;
use std::path::Path;

/// All data extracted from a file on disk, ready for storage.
#[derive(Debug, Clone)]
pub struct ExtractedData {
    /// SHA-256 content hash.
    pub hash: String,
    /// Detected media type, dimensions, and file size.
    pub media_info: MediaInfo,
    /// Serialized PNG metadata JSON (ComfyUI prompt/workflow), if applicable.
    pub metadata_json: Option<String>,
    /// File creation timestamp as an RFC 3339 string.
    pub created_at_iso: String,
    /// File modification timestamp as an RFC 3339 string.
    pub modified_at_iso: String,
    /// File name (last path component).
    pub filename: String,
}

/// Extract all disk-level data from a file path.
///
/// This is a pure async I/O function — no database or Tantivy access.
/// It performs the following operations:
///
/// 1. Computes a SHA-256 hash of the file contents.
/// 2. Detects media type, dimensions, and file size.
/// 3. Extracts PNG tEXt/iTXt metadata (ComfyUI prompt/workflow) if the file
///    is a PNG.
/// 4. Reads filesystem creation and modification timestamps.
/// 5. Extracts the file name from the path.
pub async fn extract_file_data(
    path: &Path,
) -> Result<ExtractedData, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let hash = compute_file_hash(path).await?;
    let media_info = detect_media(path).await?;

    // Extract ComfyUI metadata for PNG files.
    let metadata_json = if media_info.mime_type == "image/png" {
        parse_png_metadata(path).ok().and_then(|meta| {
            if meta.prompt.is_some() || meta.workflow.is_some() {
                serde_json::to_string(&meta).ok()
            } else {
                None
            }
        })
    } else {
        None
    };

    // Read file timestamps from the filesystem.
    let disk_metadata = tokio::fs::metadata(path).await?;
    let created_at_iso = disk_metadata
        .created()
        .or_else(|_| disk_metadata.modified())
        .map(system_time_to_iso)
        .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());
    let modified_at_iso = disk_metadata
        .modified()
        .map(system_time_to_iso)
        .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());

    let filename = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

    Ok(ExtractedData { hash, media_info, metadata_json, created_at_iso, modified_at_iso, filename })
}
