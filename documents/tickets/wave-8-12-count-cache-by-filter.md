# Wave 8.12 — Key the Total-Count Cache by Mime Filter; Keep the Lock Off the Query

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 12 |
| **Estimate** | 1 hour |
| **Depends on** | — |
| **Parallel** | Yes (8.16 builds on this; 8.22 follows it) |
| **Source** | Code review §3 P4 (🟡) |

---

## Overview

`backend/src/routes/media/list.rs:76-96` has two problems:

1. The 30s cache covers only the **unfiltered** count. A mime-filtered page load runs `COUNT(*) … WHERE mime_type LIKE ?` on **every request** — a full index scan per page on 1M rows (and SQLite won't use `idx_media_mime` for a case-insensitive `LIKE` prefix by default).
2. The unfiltered `COUNT(*)` is executed **while holding** the `std::sync::Mutex` guard (list.rs:84-95): concurrent list requests block on a std mutex held across a potentially long query, on the async runtime.

Fix: key the cache by the mime filter (tiny map with the same 30s TTL) and compute the count **before** locking — the mutex is only held to store the result.

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §3 P4
- `backend/src/routes/media/list.rs:76-96` — cache + lock structure
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/routes/media/list.rs       # per-filter count cache; lock discipline fixed
backend/src/routes/media/list_test.rs  # cache-behavior tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Two identical filtered list requests within 30s execute the `COUNT(*)` query only once
- [ ] Different mime filters maintain independent cache entries (filter A does not poison filter B)
- [ ] Cache entries expire after 30s (next request re-counts and sees new data)
- [ ] The `COUNT(*)` query never executes while the mutex is held (structure criterion; see test approach)
- [ ] Cache invalidation on SSE/indexing events keeps existing behavior (if the current code invalidates, preserve it; if it relies on TTL only, keep TTL only)
- [ ] Existing list endpoint tests (3.4) pass unchanged
- [ ] `cargo test` green

## Implementation Notes

```rust
static COUNT_CACHE: Lazy<Mutex<HashMap<String, (i64, Instant)>>> = ...; // key: filter string or ""

fn cached_count(conn: &Connection, filter: Option<&str>) -> Result<i64> {
    let key = filter.unwrap_or("").to_ascii_lowercase();
    if let Some((v, at)) = COUNT_CACHE.lock().unwrap().get(&key) {
        if at.elapsed() < TTL { return Ok(*v); }
    }
    let count = run_count_query(conn, filter)?;   // <-- no lock held here
    COUNT_CACHE.lock().unwrap().insert(key, (count, Instant::now()));
    Ok(count)
}
```

- Keep the same 30s TTL constant; extract it as a named const.
- The brief second lock (store) is fine — it never wraps I/O.

## Test Strategy

- Unit/integration: request filtered list twice → assert (via a query-count helper or `sqlite3_trace`-style counter in `cfg(test)`) one COUNT executed; change filter → new COUNT; time-travel 31s (inject `Instant` or use a testable clock) → re-count.
- Concurrency smoke: 16 parallel filtered requests on a cold cache → all 200, correct count, no deadlock.
