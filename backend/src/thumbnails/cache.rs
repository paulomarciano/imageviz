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
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use dashmap::DashMap;
use std::time::SystemTime;

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
// Eviction configuration
// ---------------------------------------------------------------------------

/// Default maximum cache size in bytes (2 GB).
const DEFAULT_MAX_CACHE_SIZE: u64 = 2_000_000_000;
/// Default minimum free disk space in bytes (500 MB).
const DEFAULT_MIN_FREE_SPACE: u64 = 500_000_000;
/// Target usage ratio after eviction: evict down to 80% of max.
const EVICTION_TARGET_RATIO: f64 = 0.8;

/// Get the max cache size from `THUMBNAIL_CACHE_MAX_MB` env var (in MB).
pub fn max_cache_size() -> u64 {
    std::env::var("THUMBNAIL_CACHE_MAX_MB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|mb| mb * 1_024 * 1_024)
        .unwrap_or(DEFAULT_MAX_CACHE_SIZE)
}

/// Get the minimum free disk space from `MIN_FREE_DISK_MB` env var (in MB).
pub fn min_free_disk_space() -> u64 {
    std::env::var("MIN_FREE_DISK_MB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|mb| mb * 1_024 * 1_024)
        .unwrap_or(DEFAULT_MIN_FREE_SPACE)
}

/// Statistics from a single eviction pass.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EvictionStats {
    /// Number of files evicted.
    pub evicted: usize,
    /// Total bytes freed.
    pub freed_bytes: u64,
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

static LOCKS: LazyLock<DashMap<String, Arc<tokio::sync::Mutex<()>>>> = LazyLock::new(DashMap::new);

/// Acquire or create a per-key mutex for the given cache key.
///
/// Uses a global `DashMap` keyed by cache key — sharded lock design means
/// lookups and insertions are concurrent-safe without a global mutex.
///
/// The first caller to acquire the lock for a given key proceeds to generate
/// the thumbnail; subsequent callers block and then find the cached file after
/// the lock is released.
fn acquire_lock(key: &str) -> Arc<tokio::sync::Mutex<()>> {
    LOCKS
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .value()
        .clone()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Retrieve a thumbnail from the content-addressed cache, generating it if absent.
///
/// # Arguments
/// * `source_path` — Path to the source media file.
/// * `checksum` — Full SHA-256 hex digest of the source file content.
/// * `target_width` — Desired thumbnail width in pixels.
/// * `cache_dir` — Root directory for the on-disk cache.
/// * `mime_type` — MIME type of the source (e.g. `image/png`, `video/mp4`).
///   Used to select the appropriate generation strategy: the `image` crate for
///   images, ffmpeg + `image` crate for videos.
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
    mime_type: &str,
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

    // Generate the thumbnail.
    // For video files: extract a PNG keyframe via ffmpeg, then convert to WebP.
    // For images: resize the source to WebP directly.
    let generated_path = if mime_type.starts_with("video/") {
        let video_temp = std::env::temp_dir().join("imageviz-video-thumbs");
        tokio::fs::create_dir_all(&video_temp).await?;

        let frame_path = super::video::extract_video_thumbnail(
            source_path,
            &video_temp,
            1, // extract frame at 1 second
        )
        .await
        .map_err(|e| CacheError::Generation(e.to_string()))?;

        // Convert the extracted PNG frame to a WebP thumbnail at the
        // requested width.
        let webp_path = image::generate_image_thumbnail(&frame_path, target_width)
            .await
            .map_err(|e| CacheError::Generation(e.to_string()))?;

        // Clean up the intermediate frame PNG.
        let _ = tokio::fs::remove_file(&frame_path).await;

        webp_path
    } else {
        image::generate_image_thumbnail(source_path, target_width)
            .await
            .map_err(|e| CacheError::Generation(e.to_string()))?
    };

    // Atomic write: copy to a temp file inside the cache directory, then
    // rename (which is atomic on the same filesystem).
    let tmp_path = cache_dir.join(format!("{key}.tmp"));
    tokio::fs::copy(&generated_path, &tmp_path).await?;
    tokio::fs::rename(&tmp_path, &cache_path).await?;

    // Check cache size and evict old files if needed (best-effort, fire-and-forget).
    // The primary eviction is handled by a background timer (see
    // `spawn_cache_eviction_timer` in main.rs).  This inline spawn is an
    // extra safety net for unusually large cache bursts.
    let cache_dir = cache_dir.to_path_buf();
    // Fire-and-forget: the background timer in main.rs is the primary eviction
    // mechanism. This spawn is an extra safety net for large caches.
    std::mem::drop(tokio::spawn(async move {
        if let Err(e) = evict_if_needed(&cache_dir, max_cache_size(), min_free_disk_space()) {
            tracing::warn!(error = %e, "Cache eviction check failed");
        }
    }));

    Ok(cache_path)
}

// ---------------------------------------------------------------------------
// Eviction logic
// ---------------------------------------------------------------------------

/// Compute the total size of all files in a directory (shallow, non-recursive).
fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}

/// Estimate free disk space for the filesystem containing `path`.
///
/// Uses the `fs2` crate's cross-platform `available_space()` function which
/// calls `statvfs` on Linux and `statfs` on macOS. Falls back to `u64::MAX`
/// (no-op) if the platform API returns an error, and logs a warning so that
/// operators can detect the silent fallback.
fn free_disk_space(path: &Path) -> u64 {
    fs2::available_space(path).unwrap_or_else(|e| {
        tracing::warn!(
            error = %e,
            path = %path.display(),
            "Failed to query available disk space — eviction will be disabled"
        );
        u64::MAX
    })
}

/// Check cache size and evict old files if the cache exceeds the limit or
/// disk space is low.
///
/// # Eviction strategy
///
/// 1. Gather all cached files with their access times and sizes.
/// 2. Sort by access time (oldest first).
/// 3. Compute target: evict enough to bring cache to 80% of max size.
/// 4. If free disk space is below the minimum, evict even more.
/// 5. Delete files oldest-first until the target is reached.
///
/// # Errors
///
/// Returns `CacheError::Io` for filesystem errors. Individual file deletion
/// failures are logged as warnings and skipped — the eviction continues with
/// the next file.
pub fn evict_if_needed(
    cache_dir: &Path,
    max_size_bytes: u64,
    min_free_bytes: u64,
) -> Result<EvictionStats, CacheError> {
    if !cache_dir.exists() {
        return Ok(EvictionStats::default());
    }

    let current_size = dir_size(cache_dir)?;
    let free_space = free_disk_space(cache_dir);

    if current_size <= max_size_bytes && free_space >= min_free_bytes {
        return Ok(EvictionStats::default());
    }

    // Gather all cached files with their metadata.
    let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    for entry in std::fs::read_dir(cache_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let metadata = entry.metadata()?;
            files.push((
                entry.path(),
                metadata.len(),
                metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH),
            ));
        }
    }

    // Sort by access time (oldest first).
    files.sort_by_key(|(_, _, atime)| *atime);

    // Calculate how much to free.
    let target_size = (max_size_bytes as f64 * EVICTION_TARGET_RATIO) as u64;
    let mut to_free = current_size.saturating_sub(target_size);

    // If free space is critically low, evict more aggressively.
    if free_space < min_free_bytes {
        to_free = to_free.max(min_free_bytes.saturating_sub(free_space));
    }

    if to_free == 0 {
        return Ok(EvictionStats::default());
    }

    let mut evicted = 0;
    let mut freed_bytes = 0u64;

    for (path, size, _) in &files {
        if freed_bytes >= to_free {
            break;
        }
        if let Err(e) = std::fs::remove_file(path) {
            tracing::warn!(path = %path.display(), error = %e, "Failed to evict thumbnail");
            continue;
        }
        evicted += 1;
        freed_bytes += size;
    }

    tracing::info!(
        evicted,
        freed_mb = freed_bytes / (1024 * 1024),
        cache_mb = current_size / (1024 * 1024),
        max_mb = max_size_bytes / (1024 * 1024),
        "Evicted {} thumbnails, freed {} MB (cache: {} MB / {} MB)",
        evicted,
        freed_bytes / (1024 * 1024),
        current_size / (1024 * 1024),
        max_size_bytes / (1024 * 1024),
    );

    Ok(EvictionStats { evicted, freed_bytes })
}

#[cfg(test)]
#[path = "cache_test.rs"]
mod tests;
