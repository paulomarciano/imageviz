# Wave 2.3 — Implement Thumbnail Cache (On-Disk, Content-Addressed)

| Field | Value |
|-------|-------|
| **Wave** | 2 — Backend: Thumbnail Generation & Media Serving |
| **Seq** | 03 |
| **Estimate** | 1.5 hours |
| **Depends on** | 2.1 (image thumbnails), 2.2 (video thumbnails) |
| **Parallel** | No (wraps both generators) |

---

## Overview

Implement an on-disk thumbnail cache using content-addressed storage. Thumbnails are keyed by the source file's SHA-256 hash — if the source file hasn't changed, the cached thumbnail is reused. This avoids re-generating thumbnails on every request.

## Prerequisites

- Image thumbnail generator (2.1)
- Video thumbnail extractor (2.2)
- File hash computation (1.7)

## Reference Files

- `documents/plans/development-plan.md` — §8.2 Key Performance Decisions (content-addressed cache), §8.3 Memory Management, §12 cache.rs module
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/thumbnails/
├── mod.rs                       # Updated: re-export cache
├── cache.rs                     # Caching layer
└── cache_test.rs                # Co-located tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] `get_or_generate_thumbnail(media_id, source_path, target_width)` returns path to thumbnail
- [ ] First request: generates thumbnail, stores on disk, returns path
- [ ] Second request for same file (unchanged): returns cached thumbnail instantly — **no regeneration**
- [ ] Cache keyed by file checksum + target width (e.g., `cache/{checksum}_{width}.webp`)
- [ ] Cache directory is configurable (default: `~/.imageviz/thumbnails/` or `./data/thumbnails/`)
- [ ] Returns error if source file doesn't exist
- [ ] Thread-safe (can handle concurrent requests for the same thumbnail)

## Implementation Notes

**Cache key strategy:**
```
{checksum}_{target_width}.webp
```
Example: `a1b2c3d4...e5f6_200.webp`

This ensures:
- Same file → same checksum → same thumbnail → cache hit
- Different target width → different filename → separate cache entry
- File modified → different checksum → new cache entry (old one can be cleaned up by eviction in Wave 7.4)

**Cache module:**
```rust
use std::path::{Path, PathBuf};

pub struct ThumbnailCache {
    cache_dir: PathBuf,
    thumbnail_generator: ThumbnailGenerator, // Wraps image + video generators
}

impl ThumbnailCache {
    pub fn new(cache_dir: PathBuf) -> Self { ... }

    pub async fn get_or_generate(
        &self,
        source_path: &Path,
        source_checksum: &str,
        target_width: u32,
    ) -> Result<PathBuf, Error> {
        let cache_filename = format!("{}_{}.webp", source_checksum, target_width);
        let cache_path = self.cache_dir.join(&cache_filename);
        
        // Cache hit
        if cache_path.exists() {
            return Ok(cache_path);
        }
        
        // Generate
        let thumb_path = self.thumbnail_generator
            .generate(source_path, &self.cache_dir, target_width)
            .await?;
        
        // Rename to content-addressed name if generator didn't use it already
        if thumb_path != cache_path {
            tokio::fs::rename(&thumb_path, &cache_path).await?;
        }
        
        Ok(cache_path)
    }
}
```

**Concurrency handling:**
Multiple requests for the same thumbnail could arrive simultaneously (before the first generation completes). Use a concurrent request deduplication map:
```rust
use std::collections::HashMap;
use tokio::sync::Mutex;

struct InFlightRequests {
    map: Mutex<HashMap<String, tokio::sync::oneshot::Receiver<()>>>,
}

// When a generation starts, insert a oneshot channel
// Concurrent requests wait on the same channel
// When generation completes, signal all waiters
```

This prevents N concurrent ffmpeg processes for the same video.

**Cache directory:**
Default to a platform-appropriate location:
- Linux: `~/.local/share/imageviz/thumbnails/` or `./data/thumbnails/`
- macOS: `~/Library/Caches/imageviz/thumbnails/`
- Configurable via env var: `IMAGEVIZ_CACHE_DIR`

## Test Strategy

```rust
#[tokio::test]
async fn test_cache_hit_on_second_request() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let cache = ThumbnailCache::new(cache_dir);
    
    let source = create_test_image(dir.path().join("test.png"), 400, 300);
    let checksum = compute_file_hash(&source).await.unwrap();
    
    // First request — should generate
    let path1 = cache.get_or_generate(&source, &checksum, 200).await.unwrap();
    let modified1 = std::fs::metadata(&path1).unwrap().modified().unwrap();
    
    // Second request — should be cached (no regeneration)
    let path2 = cache.get_or_generate(&source, &checksum, 200).await.unwrap();
    let modified2 = std::fs::metadata(&path2).unwrap().modified().unwrap();
    
    assert_eq!(path1, path2);
    assert_eq!(modified1, modified2); // Same file, not regenerated
}

#[tokio::test]
async fn test_different_width_different_cache_entry() {
    // Request at 200px and 400px → two different cache files
}
```
