//! Content-addressed on-disk thumbnail cache.
//!
//! Provides [`get_or_generate_thumbnail`] which checks a cache directory for an
//! existing thumbnail matching the given content checksum and width, or generates
//! it on demand. Cache keys are purely content-addressed (no mtime or path in the
//! key), ensuring the same content always maps to the same cache entry.
//!
//! # Thread safety
//! A per-key [`tokio::sync::Mutex`] ensures that concurrent calls for the same
//! cache key serialise the generation step, so the underlying image processing
//! is only performed once.

use crate::thumbnails::image;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

/// Errors that can occur during cache operations.
#[derive(Debug)]
pub enum CacheError {
    /// Wraps standard I/O errors (directory creation, file copy, rename, etc.).
    Io(std::io::Error),
    /// The source file does not exist at the given path.
    SourceNotFound(PathBuf),
    /// Thumbnail generation failed; wraps the inner error message.
    Generation(String),
    /// Requested width is outside the valid range.
    InvalidWidth {
        /// The requested width.
        width: u32,
        /// Minimum allowed width.
        min: u32,
        /// Maximum allowed width.
        max: u32,
    },
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::Io(e) => write!(f, "Cache I/O error: {e}"),
            CacheError::SourceNotFound(path) => {
                write!(f, "Source not found: {}", path.display())
            }
            CacheError::Generation(msg) => write!(f, "Cache generation error: {msg}"),
            CacheError::InvalidWidth { width, min, max } => {
                write!(f, "Invalid width {width}: must be between {min} and {max}")
            }
        }
    }
}

impl std::error::Error for CacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CacheError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CacheError {
    fn from(e: std::io::Error) -> Self {
        CacheError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Cache key helpers
// ---------------------------------------------------------------------------

/// Build a cache file name: `{checksum_prefix}_{width}.webp`.
///
/// Only the first 16 characters of the checksum are used so that file names
/// stay readable while remaining collision-resistant for practical purposes.
fn cache_key(checksum: &str, target_width: u32) -> String {
    let prefix = &checksum[..checksum.len().min(16)];
    format!("{prefix}_{target_width}.webp")
}

/// Resolve the full on-disk path for a cache entry.
fn cache_file_path(cache_dir: &Path, checksum: &str, target_width: u32) -> PathBuf {
    cache_dir.join(cache_key(checksum, target_width))
}

// ---------------------------------------------------------------------------
// Per-key lock map
// ---------------------------------------------------------------------------

type LockMap = Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>;

static LOCKS: LazyLock<LockMap> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Acquire or create a per-key mutex for the given cache key.
///
/// The first caller to acquire the lock for a given key proceeds to generate
/// the thumbnail; subsequent callers block and then find the cached file after
/// the lock is released.
fn acquire_lock(key: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut map = LOCKS.lock().expect("cache lock map poisoned");
    map.entry(key.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Retrieve a thumbnail from the content-addressed cache, generating it if absent.
///
/// # Arguments
/// * `source_path` — Path to the source image file.
/// * `checksum` — Full SHA-256 hex digest of the source file content.
/// * `target_width` — Desired thumbnail width in pixels.
/// * `cache_dir` — Root directory for the on-disk cache.
///
/// # Cache key
/// `{checksum[:16]}_{target_width}.webp` — purely content-addressed via checksum
/// (no file mtime or path is incorporated).
///
/// # Thread safety
/// A per-key [`tokio::sync::Mutex`] guarantees that concurrent calls with the
/// same cache key only perform generation once. The first caller generates the
/// thumbnail; subsequent callers block on the lock, then find and return the
/// cached file directly.
///
/// # Errors
/// Returns [`CacheError::InvalidWidth`] when `target_width` is outside the
/// supported range; [`CacheError::SourceNotFound`] when the source file does
/// not exist; [`CacheError::Io`] for filesystem I/O failures; and
/// [`CacheError::Generation`] when the underlying thumbnail generation fails.
pub async fn get_or_generate_thumbnail(
    source_path: &Path,
    checksum: &str,
    target_width: u32,
    cache_dir: &Path,
) -> Result<PathBuf, CacheError> {
    // --- Validate width range (fail fast) ---
    if !(image::MIN_WIDTH..=image::MAX_WIDTH).contains(&target_width) {
        return Err(CacheError::InvalidWidth {
            width: target_width,
            min: image::MIN_WIDTH,
            max: image::MAX_WIDTH,
        });
    }

    let key = cache_key(checksum, target_width);
    let cache_path = cache_file_path(cache_dir, checksum, target_width);

    // Fast path: cache hit — return immediately without any lock.
    if cache_path.exists() {
        return Ok(cache_path);
    }

    // Validate source existence before acquiring the per-key lock.
    if !source_path.exists() {
        return Err(CacheError::SourceNotFound(source_path.to_path_buf()));
    }

    // Acquire per-key lock so concurrent callers serialise generation.
    let lock = acquire_lock(&key);
    let _guard = lock.lock().await;

    // Double-check: another task may have populated the cache while we waited.
    if cache_path.exists() {
        return Ok(cache_path);
    }

    // Ensure the cache directory exists.
    tokio::fs::create_dir_all(cache_dir).await?;

    // Generate the thumbnail (returns a temp path in the system temp dir).
    let generated_path = image::generate_image_thumbnail(source_path, target_width)
        .await
        .map_err(|e| CacheError::Generation(e.to_string()))?;

    // Atomic write: copy to a temp file inside the cache directory, then
    // rename (which is atomic on the same filesystem).
    let tmp_path = cache_dir.join(format!("{key}.tmp"));
    tokio::fs::copy(&generated_path, &tmp_path).await?;
    tokio::fs::rename(&tmp_path, &cache_path).await?;

    Ok(cache_path)
}

#[cfg(test)]
#[path = "cache_test.rs"]
mod tests;
