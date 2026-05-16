# Wave 7.3 — Add Concurrency Limiting for Thumbnail Generation

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 03 |
| **Estimate** | 1 hour |
| **Depends on** | 2.3 (thumbnail cache) |
| **Parallel** | Can run in parallel with other Wave 7 tasks |

---

## Overview

Add a concurrency limiter (semaphore) for thumbnail generation. Since thumbnail generation uses `spawn_blocking` for CPU-bound image processing, too many concurrent generation requests can overwhelm the CPU. Limit to N concurrent generations (default: 4).

## Prerequisites

- Thumbnail cache with generation (2.3)
- `tokio::sync::Semaphore`

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 7 task 7.3, §8.3 Memory Management (spawn_blocking for CPU-bound work)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/thumbnails/
└── limiter.rs                   # Semaphore-based concurrency limiter
```

## Acceptance Criteria (Pass/Fail)

- [ ] `ThumbnailLimiter` wraps a `tokio::sync::Semaphore` with configurable permits
- [ ] Default permit count: 4 (configurable via env var)
- [ ] `acquire()` returns a permit guard that auto-releases on drop
- [ ] If all permits are in use, subsequent requests wait (blocking the handler, not the async runtime)
- [ ] Timeout on acquire: if permit not available within 120s, return error
- [ ] Permits released gracefully on drop (even if generation fails)
- [ ] Used in thumbnail serving endpoint (2.4) and cache (2.3)

## Implementation Notes

```rust
use tokio::sync::{Semaphore, SemaphorePermit, AcquireError};
use std::sync::Arc;
use std::time::Duration;

pub struct ThumbnailLimiter {
    semaphore: Arc<Semaphore>,
    timeout: Duration,
}

impl ThumbnailLimiter {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            timeout: Duration::from_secs(120),
        }
    }

    pub async fn acquire(&self) -> Result<ThumbnailPermit, AcquireError> {
        match tokio::time::timeout(self.timeout, self.semaphore.acquire()).await {
            Ok(Ok(permit)) => {
                Ok(ThumbnailPermit {
                    _permit: permit,
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AcquireError::closed()), // Timeout
        }
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

pub struct ThumbnailPermit {
    _permit: SemaphorePermit<'static>,
    // The 'static lifetime is a lie — this is safe because the Semaphore is in an Arc
    // In practice, use unsafe transmute or OwnedSemaphorePermit
}

// Better approach: use Arc<Semaphore> with tokio's owned permit
impl ThumbnailLimiter {
    pub async fn acquire_owned(self: Arc<Self>) -> Result<OwnedThumbnailPermit, AcquireError> {
        match tokio::time::timeout(
            self.timeout,
            self.semaphore.clone().acquire_owned(),
        ).await {
            Ok(Ok(permit)) => Ok(OwnedThumbnailPermit { _permit: permit }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AcquireError::closed()),
        }
    }
}

pub struct OwnedThumbnailPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
}
```

**Integration with thumbnail cache:**
```rust
// In thumbnail cache or serving endpoint
async fn get_thumbnail(
    State(state): State<Arc<AppState>>,
    // ...
) -> Result<impl IntoResponse, AppError> {
    // Acquire permit before generating
    let _permit = state.thumbnail_limiter.acquire_owned().await
        .map_err(|_| AppError::ServiceUnavailable("Too many thumbnail requests. Try again later.".into()))?;
    
    // Generate thumbnail (permit held during entire generation)
    let thumb_path = state.thumbnail_cache.get_or_generate(...).await?;
    
    // Permit released when `_permit` is dropped
    // ...
}
```

**Configuration:**
```rust
// In settings.rs
pub fn max_thumbnail_concurrency() -> usize {
    std::env::var("THUMBNAIL_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4)
}
```

## Test Strategy

```rust
#[tokio::test]
async fn test_limiter_acquire_and_release() {
    let limiter = Arc::new(ThumbnailLimiter::new(2));
    
    // Acquire two permits
    let p1 = limiter.clone().acquire_owned().await.unwrap();
    let p2 = limiter.clone().acquire_owned().await.unwrap();
    
    assert_eq!(limiter.available_permits(), 0);
    
    // Third acquisition should timeout (if timeout is short)
    drop(p1); // Release first permit
    assert_eq!(limiter.available_permits(), 1);
    
    // Now can acquire again
    let p3 = limiter.acquire_owned().await.unwrap();
    assert_eq!(limiter.available_permits(), 0);
}
```
