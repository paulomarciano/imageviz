# Wave 8.9 — Acquire the Thumbnail Semaphore Only on Cache Miss

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 09 |
| **Estimate** | 1 hour |
| **Depends on** | 8.7 (route file also touched), 8.8 (cache.rs settled) |
| **Parallel** | No |
| **Source** | Code review §3 P5 (🟡) |

---

## Overview

`thumbnail_limiter.acquire()` (`backend/src/routes/media/thumbnail.rs:46-51`, 4 permits) is taken **before** the cache lookup. A grid of cached thumbnails is therefore served at most 4-at-a-time, and cache hits queue behind in-flight CPU-bound video generations. The cache's own per-key locking already prevents duplicate generation, so the limiter only needs to guard the expensive generation step.

Fix: probe the cache first; acquire a permit only on a miss. Cached responses bypass the limiter entirely.

## Prerequisites

- 8.7 (thumbnail.rs write removal landed — same handler)
- 8.8 (cache.rs stabilized)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §3 P5
- `backend/src/routes/media/thumbnail.rs:34-51` — handler + limiter ordering
- `backend/src/thumbnails/cache.rs` — cache probe API
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/routes/media/thumbnail.rs    # cache probe before acquire; permit scoped to generation
backend/src/thumbnails/cache.rs          # expose a cheap probe (e.g. cache_path(key).exists()) if not already
backend/tests/                           # limiter-bypass test
```

## Acceptance Criteria (Pass/Fail)

- [ ] Cache hits do not consume semaphore permits (test: >4 concurrent cache-hit requests all served without queueing)
- [ ] Cache misses remain bounded by `THUMBNAIL_CONCURRENCY`
- [ ] Concurrent misses for the same key still produce exactly one generation (dedup tests pass)
- [ ] No TOCTOU hazard: a request that sees "missing" and then waits on the per-key lock re-checks the cache after acquiring it (hit-inside-lock is served without generating)
- [ ] Response headers/ETag behavior unchanged
- [ ] `cargo test` green

## Implementation Notes

- Probe = a plain `cache_dir.join(key).exists()` (or the cache struct's existing path helper) — cheap, no scan.
- Structure:

```rust
let cache_path = cache.probe(key);
if let Some(p) = cache_path { return serve(p); }
let _permit = thumbnail_limiter.acquire().await;
let path = cache.get_or_generate(...).await?; // re-checks cache under the per-key lock
```

- The double-check inside `get_or_generate` already exists (per-key mutex + cache re-lookup) — rely on it; do not add a second mechanism.

## Test Strategy

- Concurrency test with `THUMBNAIL_CONCURRENCY=2`: 8 concurrent requests for 8 *cached* thumbnails complete without serializing (assert wall-time or permit-acquisition counter).
- Same test for 8 *uncached* thumbnails: at most 2 generations in flight (existing generation-counter test from 7.3 reused).
- Re-run 2.3/2.4 integration suites.
