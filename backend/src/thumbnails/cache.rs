//! Content-addressed on-disk thumbnail cache.
//!
//! Provides [`get_or_generate_thumbnail`] which checks a cache directory for an
//! existing thumbnail matching the given content checksum and width, or generates
//! it on demand. Cache keys are purely content-addressed (no mtime or path in the
//! key), ensuring the same content always maps to the same cache entry.
//!
//! # Thread safety
//! A per-checksum [`tokio::sync::Mutex`] ensures that concurrent calls for the
//! same content (at any width) serialise the generation step, so the underlying
//! image processing is only performed once. Lock entries are held as `Weak`
//! references and evicted when the last in-flight generation for a checksum
//! finishes, keeping the lock map bounded by concurrent work rather than by
//! total library size.

use crate::thumbnails::image;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Weak};

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
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

/// Default maximum cache size in bytes (~1.86 GiB / 2 GB decimal).
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

/// Per-content generation locks, keyed by [`lock_key`] (`checksum[:16]`).
///
/// Values are `Weak` references so the map never keeps a lock alive on its
/// own: an entry exists exactly as long as some request is holding (or
/// waiting on) that lock. [`KeyLockGuard`] evicts the entry on drop, which
/// bounds the map by in-flight generations instead of total library size.
///
/// Invariant: strong references to the mapped mutexes exist only inside a
/// [`KeyLockGuard`] — any `Arc::clone` escaping the guard breaks eviction.
static LOCKS: LazyLock<DashMap<String, Weak<tokio::sync::Mutex<()>>>> = LazyLock::new(DashMap::new);

/// Lock-map key for a content checksum: its first 16 characters.
///
/// Deliberately excludes the target width so that concurrent requests for
/// different widths of the same file share one lock entry — the map grows
/// with unique content, not unique (content, width) pairs.
fn lock_key(checksum: &str) -> &str {
    &checksum[..checksum.len().min(16)]
}

/// RAII guard for a per-content generation lock.
///
/// Holds the owned mutex guard (which also keeps the mutex alive). On drop
/// it evicts the map entry iff no other request still holds a strong
/// reference to the same mutex.
struct KeyLockGuard {
    key: String,
    /// Owned mutex guard; also holds the strong `Arc<Mutex>` reference.
    _mutex_guard: tokio::sync::OwnedMutexGuard<()>,
}

impl Drop for KeyLockGuard {
    fn drop(&mut self) {
        // `strong_count() == 1` means only this guard still references the
        // mutex (the map itself stores a `Weak`), so the entry is dead weight.
        LOCKS.remove_if(&self.key, |_, weak| weak.strong_count() == 1);
    }
}

/// Acquire the per-content generation lock for `key`, creating the entry if
/// absent.
///
/// The dashmap `Entry` API keeps lookup/insert/replace atomic per shard: a
/// live entry is reused via `Weak::upgrade`; a dead one (its holders dropped
/// between an eviction and the next acquire) is replaced in place. No retry
/// loop is needed because every branch returns while the shard lock is held,
/// and the entry guard is never held across the `.await` below.
async fn acquire_lock(key: &str) -> KeyLockGuard {
    let mutex = match LOCKS.entry(key.to_string()) {
        Entry::Occupied(mut occupied) => {
            if let Some(existing) = occupied.get().upgrade() {
                existing
            } else {
                let fresh = Arc::new(tokio::sync::Mutex::new(()));
                *occupied.get_mut() = Arc::downgrade(&fresh);
                fresh
            }
        }
        Entry::Vacant(vacant) => {
            let fresh = Arc::new(tokio::sync::Mutex::new(()));
            vacant.insert(Arc::downgrade(&fresh));
            fresh
        }
    };
    let mutex_guard = Arc::clone(&mutex).lock_owned().await;
    KeyLockGuard { key: key.to_string(), _mutex_guard: mutex_guard }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Probe the cache for an existing thumbnail without generating it.
///
/// Cheap and read-only: a single `stat` of the content-addressed cache entry.
/// Used by the thumbnail route to serve cache hits without acquiring a
/// generation permit (wave 8.9 / review P5). Returns the cached file path if
/// present, `None` otherwise. Never creates files or directories.
///
/// Unlike [`get_or_generate_thumbnail`], this does *not* validate
/// `target_width` — callers must validate first and treat `None` uniformly.
pub fn probe_thumbnail(cache_dir: &Path, checksum: &str, target_width: u32) -> Option<PathBuf> {
    let path = cache_file_path(cache_dir, checksum, target_width);
    path.exists().then_some(path)
}

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
/// A per-checksum [`tokio::sync::Mutex`] guarantees that concurrent calls for
/// the same content (at any width) only perform generation once. The first
/// caller generates the thumbnail; subsequent callers block on the lock, then
/// find and return the cached file directly. The lock entry is evicted once
/// the last in-flight call for that checksum finishes.
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

    // Acquire the per-checksum lock so concurrent callers serialise
    // generation for the same content (across all widths). The guard evicts
    // the lock entry on drop, including on early returns and errors.
    let _guard = acquire_lock(lock_key(checksum)).await;

    // Double-check: another task may have populated the cache while we waited.
    if cache_path.exists() {
        return Ok(cache_path);
    }

    // Ensure the cache directory exists.
    tokio::fs::create_dir_all(cache_dir).await?;

    // Generate the thumbnail directly into the cache directory as `{key}.tmp`,
    // then publish it with an atomic rename onto `{key}.webp`. Nothing is
    // written to the OS temp directory. A leftover `.tmp` from a crashed
    // generation is harmless: it is overwritten here, and eviction sweeps it.
    let tmp_path = cache_dir.join(format!("{key}.tmp"));

    // For video files: extract a PNG keyframe via ffmpeg into the cache dir
    // (ffmpeg needs a file target; the `.png` extension lets the image2 muxer
    // select the PNG codec), convert it to WebP at the requested width, then
    // delete the intermediate frame.
    // For images: resize the source to WebP directly.
    if mime_type.starts_with("video/") {
        let frame_path = cache_dir.join(format!("{key}.frame.png"));
        let extracted = super::video::extract_video_thumbnail(
            source_path,
            &frame_path,
            super::video::DEFAULT_TIMESTAMP_SECS,
        )
        .await;
        if extracted.is_err() {
            // ffmpeg may leave a partial frame behind — clean it up before
            // propagating the error so the next attempt starts clean.
            let _ = tokio::fs::remove_file(&frame_path).await;
        }
        extracted.map_err(|e| CacheError::Generation(e.to_string()))?;

        let converted = image::generate_image_thumbnail(&frame_path, target_width, &tmp_path).await;
        // Clean up the intermediate frame regardless of conversion outcome.
        let _ = tokio::fs::remove_file(&frame_path).await;
        converted.map_err(|e| CacheError::Generation(e.to_string()))?;
    } else {
        image::generate_image_thumbnail(source_path, target_width, &tmp_path)
            .await
            .map_err(|e| CacheError::Generation(e.to_string()))?;
    }

    // Atomic publish: rename is atomic on the same filesystem.
    tokio::fs::rename(&tmp_path, &cache_path).await?;

    // Eviction is deliberately NOT triggered inline here: the background
    // 5-minute timer (`spawn_cache_eviction_timer` in main.rs) is the sole
    // eviction path. An inline scan after every cache miss caused a full
    // `read_dir` + stat of the cache directory per generation (wave 8.8).

    Ok(cache_path)
}

// ---------------------------------------------------------------------------
// Eviction logic
// ---------------------------------------------------------------------------

/// Test-only instrumentation: number of `dir_size` invocations. Used by
/// `cache_test.rs` to assert that cache generations never scan the cache
/// directory (wave 8.8 / review finding R3) — eviction scans must come only
/// from the background timer.
#[cfg(test)]
pub(crate) static DIR_SIZE_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Compute the total size of all files in a directory (shallow, non-recursive).
fn dir_size(path: &Path) -> std::io::Result<u64> {
    #[cfg(test)]
    DIR_SIZE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
