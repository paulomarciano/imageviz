# Wave 8.30 — Count-Cache Hygiene: Validate the Mime Filter; Never Cache a Failed COUNT

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 30 |
| **Estimate** | 1 hour |
| **Depends on** | 8.12 (per-filter count cache) |
| **Parallel** | Yes |
| **Source** | Code review follow-up on wave-8-12 (`219ac0b`) — reviewer findings M2, M3 |

---

## Overview

Follow-ups to the wave-8-12 count cache, both confined to the list route:

1. **M2 — unvalidated filter becomes a cache key.** `mime_type` is user-supplied and,
   since 8.12, retained as a `CountCache` key for up to 30s. Every other route
   parameter is validated at the boundary (`limit` 1–500, cursor ISO 8601,
   `cursor_id` UUID, search `q` ≤ 1000, path ≤ 4096) — `mime_type` is not. Growth is
   rate-bound (expired keys are pruned on insert), but peak map size = distinct
   filters within one TTL window × key length, so a LAN client can balloon it.
   Add a length cap (≤ 100 chars) validated in `list_media` alongside the others.
2. **M3 — error-zeros are cached for the full TTL.** On a transient `query_row`
   failure, `.unwrap_or(0)` produces `0` and 8.12 caches it: clients see
   `total: 0` for up to 30s after a DB hiccup. Only successful counts should
   populate the cache; a failed COUNT returns 0 for that response but must not
   poison the window.

## Prerequisites

- Wave 8.12 merged (`219ac0b`).

## Reference Files

- `backend/src/routes/media/list.rs` — `cached_count` / `run_count` closure
- `backend/src/middleware/validation.rs` — existing validators to mirror
- `backend/src/routes/media/list_test.rs` — cache-behavior tests to extend
- `documents/tickets/wave-8-12-count-cache-by-filter.md`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/middleware/validation.rs    # validate_mime_type (≤ 100 chars)
backend/src/routes/media/list.rs       # call validator; fallible run_count; no cache on error
backend/src/routes/media/list_test.rs  # validation + no-cache-on-error tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] `mime_type` longer than 100 chars → 400, with an error message consistent with
      the other validation failures (mention the limit, like `validate_limit` does)
- [ ] Filters of ≤ 100 chars (e.g. `image/%`) behave exactly as before — existing
      list endpoint tests pass unchanged
- [ ] A failed COUNT does not populate the cache: with a query stub that errors once
      then succeeds, request #1 returns `total: 0` and runs query #2 on the next
      request (no cached zero); two consecutive successful requests run one COUNT
- [ ] Successful per-filter caching is unchanged (dedup, isolation, TTL, pruning)
- [ ] Lock discipline preserved — `try_lock` probe test still green (COUNT never
      executes while the mutex is held)
- [ ] `cargo test` green, `cargo clippy -- -D warnings` clean, `cargo fmt --check` clean

## Implementation Notes

- Make the query fallible and let `cached_count` decide what to cache:
  ```rust
  fn cached_count(
      cache: &Mutex<CountCache>,
      filter: Option<&str>,
      now: Instant,
      run_count: impl FnOnce() -> Option<i64>,   // None = query failed
  ) -> i64 {
      let key = count_cache_key(filter);
      if let Some(fresh) = fresh_count(&cache.lock().unwrap(), &key, now) {
          return fresh;
      }
      let Some(count) = run_count() else { return 0 };  // miss is NOT cached
      { /* prune + insert, as today */ }
      count
  }
  ```
  The route closure maps `query_row(...).ok()` (drop the `.unwrap_or(0)`).
- `validate_mime_type(filter: Option<&str>)` — 400 when `Some` and
  `chars().count() > 100`. Do not shape-check the pattern: the frontend sends SQL
  `LIKE` patterns (`image/%`), which are legitimate.
- Keep the response for a failed COUNT at `total: 0` (current behavior) — changing
  the status code is API-behavior churn and out of scope here.
- Update the existing `list_test.rs` query stubs to the `Option<i64>` signature.

## Test Strategy

- Unit (`list_test.rs`): stub that returns `None` once then `Some(n)` — assert first
  call runs a query and yields 0, second call runs a query again (no cached zero)
  and yields `n`, third call is served from cache (counter == 2 for the success path).
- Route-level: `mime_type` of 101 chars → 400 with limit mentioned; 100 chars → 200.
- Existing cache-behavior tests (dedup / isolation / TTL / try_lock / concurrency)
  must pass with only the signature migration.
