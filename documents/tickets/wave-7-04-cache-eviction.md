# Wave 7.4 — Implement Disk Space Monitoring for Thumbnail Cache

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 04 |
| **Estimate** | 1.5 hours |
| **Depends on** | 2.3 (thumbnail cache) |
| **Parallel** | Can run in parallel with other Wave 7 tasks |

---

## Overview

Add disk space monitoring and LRU (Least Recently Used) eviction to the thumbnail cache. When the cache exceeds a maximum size (default: 2GB), the oldest unused thumbnails are deleted to free space. This prevents the cache from growing indefinitely and consuming all available disk space.

## Prerequisites

- Thumbnail cache (2.3)
- Cache directory with content-addressed files

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 7 task 7.4
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/thumbnails/
└── cache.rs                     # Updated: LRU eviction logic
```

## Acceptance Criteria (Pass/Fail)

- [ ] Cache has a configurable max size (default: 2GB, configurable via env var)
- [ ] When cache exceeds max size, least recently accessed files are deleted
- [ ] Eviction uses file access time (`atime`) or a separate tracking mechanism
- [ ] Eviction runs after each new thumbnail is added (or periodically)
- [ ] Minimum free space check: if disk has < 500MB free, eviction is more aggressive
- [ ] Eviction log message: "Evicted N thumbnails, freed X MB"
- [ ] Cache size is tracked accurately (directory size computation)
- [ ] Empty cache doesn't crash (no files to evict)

## Implementation Notes

**LRU eviction strategy:**
Since content-addressed thumbnails have deterministic filenames, we can use file access timestamps for LRU:

```rust
use std::path::Path;
use std::fs;

pub fn evict_if_needed(cache_dir: &Path, max_size_bytes: u64, min_free_bytes: u64) -> Result<EvictionStats, Error> {
    let current_size = dir_size(cache_dir)?;
    let free_space = free_disk_space(cache_dir)?;
    
    if current_size <= max_size_bytes && free_space >= min_free_bytes {
        return Ok(EvictionStats { evicted: 0, freed_bytes: 0 });
    }
    
    // Gather all cached files with their last access times
    let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    
    for entry in fs::read_dir(cache_dir)? {
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
    
    // Sort by access time (oldest first)
    files.sort_by_key(|(_, _, atime)| *atime);
    
    // Calculate how much to free
    let target_size = (max_size_bytes as f64 * 0.8) as u64; // Evict to 80% of max
    let mut to_free = if current_size > max_size_bytes {
        current_size - target_size
    } else {
        0
    };
    
    // Also ensure minimum free space
    if free_space < min_free_bytes {
        to_free = to_free.max(min_free_bytes - free_space);
    }
    
    let mut evicted = 0;
    let mut freed_bytes = 0u64;
    
    for (path, size, _) in &files {
        if freed_bytes >= to_free {
            break;
        }
        
        if let Err(e) = fs::remove_file(path) {
            tracing::warn!("Failed to evict {}: {}", path.display(), e);
            continue;
        }
        
        evicted += 1;
        freed_bytes += size;
    }
    
    tracing::info!(
        "Evicted {} thumbnails, freed {} MB (cache: {} MB / {} MB)",
        evicted,
        freed_bytes / (1024 * 1024),
        current_size / (1024 * 1024),
        max_size_bytes / (1024 * 1024),
    );
    
    Ok(EvictionStats { evicted, freed_bytes })
}

fn dir_size(path: &Path) -> Result<u64, Error> {
    let mut total = 0u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}

fn free_disk_space(path: &Path) -> Result<u64, Error> {
    // Use fs2 or nix crate for cross-platform disk space
    // Or shell out to `df`
    // Simplified: return 1GB for now
    Ok(1_000_000_000)
}
```

**Integration with cache:**
```rust
impl ThumbnailCache {
    pub async fn get_or_generate(&self, ...) -> Result<PathBuf, Error> {
        // ... generation logic ...
        
        // After saving new thumbnail, check if eviction needed
        if let Err(e) = evict_if_needed(
            &self.cache_dir,
            self.max_cache_size,
            self.min_free_space,
        ) {
            tracing::warn!("Cache eviction check failed: {}", e);
            // Non-fatal — cache will continue to grow
        }
        
        Ok(cache_path)
    }
}
```

**Configuration:**
```rust
pub fn max_cache_size() -> u64 {
    std::env::var("THUMBNAIL_CACHE_MAX_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .map(|mb: u64| mb * 1024 * 1024)
        .unwrap_or(2_000_000_000) // 2GB default
}

pub fn min_free_disk_space() -> u64 {
    std::env::var("MIN_FREE_DISK_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .map(|mb: u64| mb * 1024 * 1024)
        .unwrap_or(500_000_000) // 500MB default
}
```

## Test Strategy

```rust
#[test]
fn test_eviction_when_over_limit() {
    let dir = tempfile::tempdir().unwrap();
    
    // Create files totaling 100MB
    for i in 0..10 {
        let path = dir.path().join(format!("thumb_{}.webp", i));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(10 * 1024 * 1024).unwrap(); // 10MB each
    }
    
    // Set max to 50MB — should evict ~5 files
    let stats = evict_if_needed(dir.path(), 50_000_000, 100_000_000).unwrap();
    
    assert!(stats.evicted >= 5);
    assert!(stats.freed_bytes >= 50_000_000);
    
    let remaining = dir_size(dir.path()).unwrap();
    assert!(remaining < 50_000_000);
}

#[test]
fn test_no_eviction_when_under_limit() {
    let dir = tempfile::tempdir().unwrap();
    // Create small files
    let stats = evict_if_needed(dir.path(), 1_000_000_000, 100_000_000).unwrap();
    assert_eq!(stats.evicted, 0);
}
```
