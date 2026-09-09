use std::path::{Path, PathBuf};

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

use crate::media_types::{is_hidden_path, is_supported_extension};
use crate::util::system_time_to_iso;

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
        .filter_entry(|e| e.depth() == 0 || !is_hidden_path(e.path()))
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
            .map(system_time_to_iso)
            .unwrap_or_default();

        let modified_at = metadata.modified().map(system_time_to_iso).unwrap_or_default();

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
