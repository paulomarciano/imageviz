//! Semaphore-based concurrency limiter for thumbnail generation.
//!
//! Thumbnail generation uses `spawn_blocking` for CPU-bound image processing.
//! Without a limiter, N concurrent requests could spawn N blocking threads,
//! overwhelming the CPU. This module provides a [`ThumbnailLimiter`] backed by
//! [`tokio::sync::Semaphore`] that caps concurrent generations.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Default maximum number of concurrent thumbnail generations.
const DEFAULT_MAX_CONCURRENT: usize = 4;

/// Timeout for acquiring a permit (2 minutes).
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(120);

/// Concurrency limiter for thumbnail generation.
///
/// Wraps a [`tokio::sync::Semaphore`] to limit the number of concurrent
/// CPU-bound thumbnail operations. Each call to [`acquire`](Self::acquire)
/// returns a permit guard that auto-releases when dropped.
#[derive(Debug)]
pub struct ThumbnailLimiter {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
}

impl ThumbnailLimiter {
    /// Create a new limiter with the given maximum concurrent permits.
    pub fn new(max_concurrent: usize) -> Self {
        Self { semaphore: Arc::new(Semaphore::new(max_concurrent)), max_concurrent }
    }

    /// Acquire a permit, waiting up to the timeout if none are available.
    ///
    /// Returns `None` if the timeout elapses before a permit is acquired.
    pub async fn acquire(self: &Arc<Self>) -> Result<ThumbnailPermit, ThumbnailLimiterError> {
        let permit = tokio::time::timeout(ACQUIRE_TIMEOUT, self.semaphore.clone().acquire_owned())
            .await
            .map_err(|_| ThumbnailLimiterError::Timeout)?
            .map_err(|_| ThumbnailLimiterError::Closed)?;

        Ok(ThumbnailPermit { _permit: permit })
    }

    /// Returns the number of available permits.
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Returns the configured max permits.
    pub fn max_permits(&self) -> usize {
        self.max_concurrent
    }
}

/// A permit guard that releases the semaphore permit on drop.
#[derive(Debug)]
pub struct ThumbnailPermit {
    _permit: OwnedSemaphorePermit,
}

/// Errors that can occur when acquiring a thumbnail generation permit.
#[derive(Debug)]
pub enum ThumbnailLimiterError {
    /// All permits are in use and the acquisition timeout elapsed.
    Timeout,
    /// The semaphore was closed (should not happen in normal operation).
    Closed,
}

impl std::fmt::Display for ThumbnailLimiterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThumbnailLimiterError::Timeout => {
                write!(f, "Timed out waiting for thumbnail generation permit")
            }
            ThumbnailLimiterError::Closed => write!(f, "Thumbnail semaphore closed"),
        }
    }
}

impl std::error::Error for ThumbnailLimiterError {}

/// Get the max concurrent thumbnail generations from the env var.
pub fn max_thumbnail_concurrency() -> usize {
    std::env::var("THUMBNAIL_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_CONCURRENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acquire_and_release() {
        let limiter = Arc::new(ThumbnailLimiter::new(2));

        let p1 = limiter.acquire().await.unwrap();
        assert_eq!(limiter.available_permits(), 1);

        let p2 = limiter.acquire().await.unwrap();
        assert_eq!(limiter.available_permits(), 0);

        drop(p1);
        assert_eq!(limiter.available_permits(), 1);

        let p3 = limiter.acquire().await.unwrap();
        assert_eq!(limiter.available_permits(), 0);

        drop(p2);
        drop(p3);
        assert_eq!(limiter.available_permits(), 2);
    }

    #[tokio::test]
    async fn test_permit_auto_releases_on_drop() {
        let limiter = Arc::new(ThumbnailLimiter::new(1));
        {
            let _permit = limiter.acquire().await.unwrap();
            assert_eq!(limiter.available_permits(), 0);
        }
        assert_eq!(limiter.available_permits(), 1);
    }

    #[tokio::test]
    async fn test_max_permits_returns_configured_value() {
        let limiter = ThumbnailLimiter::new(4);
        assert_eq!(limiter.max_permits(), 4);

        let limiter = ThumbnailLimiter::new(8);
        assert_eq!(limiter.max_permits(), 8);
    }

    #[test]
    fn test_max_thumbnail_concurrency_default() {
        let prev = std::env::var("THUMBNAIL_CONCURRENCY").ok();
        unsafe { std::env::remove_var("THUMBNAIL_CONCURRENCY") };
        assert_eq!(max_thumbnail_concurrency(), 4);
        if let Some(ref val) = prev {
            unsafe { std::env::set_var("THUMBNAIL_CONCURRENCY", val); }
        }
    }

    #[test]
    fn test_max_thumbnail_concurrency_from_env() {
        let prev = std::env::var("THUMBNAIL_CONCURRENCY").ok();
        unsafe { std::env::set_var("THUMBNAIL_CONCURRENCY", "8") };
        assert_eq!(max_thumbnail_concurrency(), 8);
        if let Some(ref val) = prev {
            unsafe { std::env::set_var("THUMBNAIL_CONCURRENCY", val); }
        } else {
            unsafe { std::env::remove_var("THUMBNAIL_CONCURRENCY"); }
        }
    }
}
