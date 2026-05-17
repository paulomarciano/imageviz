use std::path::{Path, PathBuf};
use std::time::SystemTime;

use walkdir::WalkDir;

/// A discovered media file entry from scanning a directory.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub filename: String,
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub file_size: u64,
    pub created_at: String,
    pub modified_at: String,
}

use crate::media_types::is_supported_extension;

/// Scan a folder recursively and return all supported media files.
///
/// Uses `walkdir` for efficient directory traversal. Hidden files/directories
/// (names starting with `.`) are skipped. Permission errors are logged and
/// skipped gracefully.
pub fn scan_folder(root: &Path) -> Result<Vec<FileEntry>, WalkerError> {
    if !root.exists() {
        return Err(WalkerError::PathNotFound(root.to_path_buf()));
    }

    let mut entries = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !is_hidden(e))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "Skipping unreadable entry during scan");
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        if !is_supported_extension(entry.path()) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(path = %entry.path().display(), error = %e, "Skipping file without metadata");
                continue;
            }
        };

        let filename = entry.file_name().to_string_lossy().into_owned();

        let absolute_path = entry.path().to_path_buf();

        // Compute relative path from the root
        let relative_path = absolute_path
            .strip_prefix(root)
            .unwrap_or(&absolute_path)
            .to_string_lossy()
            .into_owned();

        let file_size = metadata.len();

        let created_at = metadata
            .created()
            .or_else(|_| metadata.modified())
            .map(datetime_to_iso)
            .unwrap_or_default();

        let modified_at = metadata.modified().map(datetime_to_iso).unwrap_or_default();

        entries.push(FileEntry {
            filename,
            relative_path,
            absolute_path,
            file_size,
            created_at,
            modified_at,
        });
    }

    Ok(entries)
}

/// Check if a directory entry is hidden (name starts with `.`).
fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry.file_name().to_str().is_some_and(|s| s.starts_with('.'))
}

/// Convert a SystemTime to ISO 8601 string with sub-second precision.
fn datetime_to_iso(time: SystemTime) -> String {
    let duration = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs() as i64;
    let nsecs = duration.subsec_nanos();
    let naive = chrono::DateTime::from_timestamp(secs, nsecs).unwrap_or_default();
    naive.to_rfc3339()
}

#[derive(Debug)]
pub enum WalkerError {
    PathNotFound(PathBuf),
    Io(std::io::Error),
}

impl std::fmt::Display for WalkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalkerError::PathNotFound(p) => write!(f, "Path not found: {}", p.display()),
            WalkerError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for WalkerError {}

impl From<std::io::Error> for WalkerError {
    fn from(e: std::io::Error) -> Self {
        WalkerError::Io(e)
    }
}

#[cfg(test)]
#[path = "walker_test.rs"]
mod tests;
