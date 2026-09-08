//! Cache-behavior tests for the media-list total-count cache (wave 8.12,
//! code-review §3 P4): one `COUNT(*)` per filter per 30s window, per-filter
//! isolation, TTL expiry, lock discipline, and concurrency safety.

use std::cell::Cell;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use super::{COUNT_CACHE_TTL, CountCache, cached_count, count_cache_key};
use crate::routes::media::routes;
use crate::routes::media::tests::{seed_media_item_full, test_state};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Query stand-in that records each invocation and returns a fixed count.
fn counting_query(calls: &AtomicUsize, value: i64) -> impl FnOnce() -> i64 + '_ {
    move || {
        calls.fetch_add(1, Ordering::SeqCst);
        value
    }
}

// ---------------------------------------------------------------------------
// Cache behavior (unit level — query counter sees the real dedup)
// ---------------------------------------------------------------------------

#[test]
fn test_identical_filtered_requests_run_count_once() {
    let cache = Mutex::new(CountCache::new());
    let calls = AtomicUsize::new(0);
    let now = Instant::now();

    let first = cached_count(&cache, Some("image/%"), now, counting_query(&calls, 25));
    let second = cached_count(&cache, Some("image/%"), now, counting_query(&calls, 25));

    assert_eq!(first, 25);
    assert_eq!(second, 25, "second request must see the cached count");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "COUNT(*) must run exactly once per 30s window");
}

#[test]
fn test_different_mime_filters_maintain_independent_entries() {
    let cache = Mutex::new(CountCache::new());
    let calls = AtomicUsize::new(0);
    let now = Instant::now();

    let images = cached_count(&cache, Some("image/%"), now, counting_query(&calls, 25));
    let videos = cached_count(&cache, Some("video/%"), now, counting_query(&calls, 4));

    assert_eq!(images, 25);
    assert_eq!(videos, 4, "filter A (image) must not poison filter B (video)");

    // Both keys are now cached: repeat requests must not re-query.
    let _ = cached_count(&cache, Some("image/%"), now, counting_query(&calls, 999));
    let _ = cached_count(&cache, Some("video/%"), now, counting_query(&calls, 999));
    assert_eq!(calls.load(Ordering::SeqCst), 2, "each filter must be counted exactly once");
}

#[test]
fn test_filter_keys_are_normalized_and_isolated_from_unfiltered() {
    let cache = Mutex::new(CountCache::new());
    let calls = AtomicUsize::new(0);
    let now = Instant::now();

    assert_eq!(cached_count(&cache, None, now, counting_query(&calls, 29)), 29);
    assert_eq!(cached_count(&cache, Some("IMAGE/%"), now, counting_query(&calls, 25)), 25);

    // "IMAGE/%" must normalize onto the same entry as "image/%"...
    assert_eq!(cached_count(&cache, Some("image/%"), now, counting_query(&calls, 0)), 25);
    // ...and the unfiltered key must stay independent of any filter key.
    assert_eq!(cached_count(&cache, None, now, counting_query(&calls, 0)), 29);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn test_cache_entry_expires_after_ttl() {
    let cache = Mutex::new(CountCache::new());
    let calls = AtomicUsize::new(0);
    let t0 = Instant::now();

    assert_eq!(cached_count(&cache, Some("image/%"), t0, counting_query(&calls, 25)), 25);

    // Just inside the window: still cached.
    let almost_expired = t0 + COUNT_CACHE_TTL - Duration::from_millis(1);
    assert_eq!(
        cached_count(&cache, Some("image/%"), almost_expired, counting_query(&calls, 0)),
        25,
        "entry younger than the TTL must be served from cache"
    );

    // Past the TTL (simulated 31s): re-counts and sees new data.
    let after_ttl = t0 + COUNT_CACHE_TTL + Duration::from_secs(1);
    assert_eq!(
        cached_count(&cache, Some("image/%"), after_ttl, counting_query(&calls, 26)),
        26,
        "expired entry must trigger a fresh COUNT"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn test_expired_entries_are_pruned_on_insert() {
    let cache = Mutex::new(CountCache::new());
    let calls = AtomicUsize::new(0);
    let t0 = Instant::now();

    cached_count(&cache, Some("image/%"), t0, counting_query(&calls, 25));
    cached_count(&cache, Some("video/%"), t0, counting_query(&calls, 4));
    assert_eq!(cache.lock().unwrap().len(), 2);

    // A re-count after the TTL replaces both stale entries — the map stays
    // bounded by the filters seen within one TTL window.
    let after_ttl = t0 + COUNT_CACHE_TTL + Duration::from_secs(1);
    cached_count(&cache, Some("image/%"), after_ttl, counting_query(&calls, 26));

    let map = cache.lock().unwrap();
    assert_eq!(map.len(), 1, "expired entries must be pruned, not accumulated");
    assert!(map.contains_key(&count_cache_key(Some("image/%"))));
}

// ---------------------------------------------------------------------------
// Lock discipline (structural criterion from the ticket)
// ---------------------------------------------------------------------------

#[test]
fn test_count_query_never_runs_while_mutex_held() {
    let cache = Mutex::new(CountCache::new());
    let lock_was_held = Cell::new(false);

    let count = cached_count(&cache, Some("image/%"), Instant::now(), || {
        // If the mutex were held across the query, try_lock would fail here.
        lock_was_held.set(cache.try_lock().is_err());
        25
    });

    assert_eq!(count, 25);
    assert!(cache.try_lock().is_ok(), "mutex must be released after the call returns");
    assert!(!lock_was_held.get(), "COUNT(*) must never execute while the cache mutex is held");
}

// ---------------------------------------------------------------------------
// Concurrency smoke (ticket: 16 parallel filtered requests)
// ---------------------------------------------------------------------------

#[test]
fn test_concurrent_requests_are_correct_and_deadlock_free() {
    let cache = Arc::new(Mutex::new(CountCache::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    // A single shared `now` keeps stored timestamps monotonic across threads.
    let now = Instant::now();

    let handles: Vec<_> = (0..16)
        .map(|_| {
            let cache = Arc::clone(&cache);
            let calls = Arc::clone(&calls);
            thread::spawn(move || {
                cached_count(&cache, Some("image/%"), now, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    25
                })
            })
        })
        .collect();

    for handle in handles {
        assert_eq!(handle.join().unwrap(), 25, "every concurrent request must see the count");
    }
    assert!(calls.load(Ordering::SeqCst) >= 1, "the query must run at least once");
}

// ---------------------------------------------------------------------------
// Route-level wiring (filter isolation end-to-end)
// ---------------------------------------------------------------------------

/// Request a filtered list page and return its `meta.total`.
async fn fetch_total(app: &axum::Router, mime: &str) -> i64 {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/media?limit=100&mime_type={mime}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    body["meta"]["total"].as_i64().unwrap()
}

#[tokio::test]
async fn test_filtered_list_totals_are_independent_end_to_end() {
    let (state, _cache_dir) = test_state();
    // Asymmetric counts (3 images vs 1 video): if both filters ever shared a
    // cache entry, the totals below would flip (1 ↔ 3) and fail.
    for (i, (id, mime)) in [
        ("img-1", "image/png"),
        ("img-2", "image/webp"),
        ("img-3", "image/gif"),
        ("vid-1", "video/mp4"),
    ]
    .into_iter()
    .enumerate()
    {
        seed_media_item_full(
            &state,
            id,
            &format!("file_{i}"),
            &format!("2025-01-01/file_{i}"),
            mime,
            "chk",
            Some(10),
            Some(10),
            "2025-06-15T12:00:00",
            "2025-06-15T12:00:00",
        )
        .await;
    }

    let app = routes().with_state(state);

    assert_eq!(fetch_total(&app, "image/%").await, 3);
    assert_eq!(fetch_total(&app, "video/%").await, 1, "video total must not reuse the image entry");
    assert_eq!(
        fetch_total(&app, "image/%").await,
        3,
        "image total must not be poisoned by the video filter"
    );
    assert_eq!(fetch_total(&app, "video/%").await, 1);
}
