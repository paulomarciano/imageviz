# Wave 8.2 — Parallelize Phase-1 File Processing

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 02 |
| **Estimate** | 2 hours |
| **Depends on** | 8.1 (incremental startup — land first to avoid conflicting indexer edits) |
| **Parallel** | No |
| **Source** | Code review §3 P1 (🔴), part 2 of 2 |

---

## Overview

Phase 1 processes files one at a time (`for ff_entry in chunk { process_file_metadata(...).await }` — `backend/src/indexer/mod.rs:83-92`): hash + ffprobe for one file, then the next. On any multicore machine this under-utilizes the CPU by ~Nx. Convert the inner loop to bounded-concurrency parallel processing with `futures::stream::iter(...).buffer_unordered(N)`.

Combined with 8.1 this is the single largest win in the review — likely **10–50× faster warm starts** on large libraries.

## Prerequisites

- 8.1 merged (startup calls the incremental path; indexer edits are fresh)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §3 P1
- `backend/src/indexer/mod.rs:47-124` — chunk loop, `store_file`, stats, error accounting
- `backend/src/thumbnails/` — `ThumbnailLimiter` pattern to reuse for a concurrency cap
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/indexer/mod.rs      # buffer_unordered processing inside the chunk loop
backend/src/indexer/mod_test.rs # concurrency + determinism tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Files within a chunk are processed concurrently, bounded by a configurable limit
- [ ] Concurrency limit configurable via env (`INDEX_CONCURRENCY`), default `available_parallelism` capped at 8
- [ ] Per-chunk transaction semantics preserved (batch insert/commit behavior unchanged)
- [ ] Stats counters (`added`/`updated`/`errors`/`skipped`) remain accurate under concurrency
- [ ] End DB state is identical to the sequential implementation for the same input (determinism test)
- [ ] Progress reporting stays correct when files complete out of order
- [ ] `cargo clippy -- -D warnings` green

## Implementation Notes

```rust
use futures::stream::{self, StreamExt};

let results: Vec<_> = stream::iter(chunk)
    .map(|entry| process_file_metadata(entry.clone(), ...))
    .buffer_unordered(concurrency)
    .collect()
    .await;
```

- `process_file_metadata` must be `Send`-friendly and not hold the DB connection across the CPU-bound work: hash/ffprobe off the async path (`spawn_blocking`), DB write back on the batch path — match the existing structure.
- Deterministic ordering is **not** required within a chunk as long as the final DB state and stats match; do not introduce shared mutable counters without atomics/mutex.
- Reuse the semaphore pattern from `ThumbnailLimiter` if a hard cap beyond `buffer_unordered(N)` is wanted — `buffer_unordered(N)` alone is sufficient here.

## Test Strategy

- **Determinism**: run indexing over a fixture tree with concurrency 1 and concurrency 4; assert identical DB rows and stats.
- **Stats under concurrency**: fixture with deliberate failures (unreadable file) → error count exact.
- **Progress**: subscribe to the progress watch channel; assert final counts converge to total files.
- Existing indexer integration tests (1.8/1.10) must pass unchanged.
