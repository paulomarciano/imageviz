# Wave 8.1 — Wire Startup Indexing to the Incremental Path

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 01 |
| **Estimate** | 1.5 hours |
| **Depends on** | — |
| **Parallel** | No (first ticket of Wave 8) |
| **Source** | Code review §3 P1 (🔴), part 1 of 2 |

---

## Overview

Every app start currently calls `full_index`, which computes a SHA-256 of **every file** — the checksum-equality skip inside `store_file` only happens *after* hashing. An `incremental_index` that skips files whose size+mtime are unchanged already exists (`backend/src/indexer/mod.rs:136`) but is called by nothing except its own test. This ticket makes startup use the incremental path. For the 100K–1M file target, this is potentially hours of redundant disk I/O per restart.

**Scope note**: do *not* consolidate `full_index`/`incremental_index` here — that is ticket 8.13, which lands after the parallelization change (8.02). This ticket is the minimal wiring to unlock the win.

## Prerequisites

- v0.7.0 baseline (all waves 0–7 complete)
- No schema changes

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §3 P1
- `backend/src/main.rs:320` — startup call site (`full_index`)
- `backend/src/indexer/mod.rs:47-124` (`full_index`), `136-237` (`incremental_index`)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/main.rs             # startup invokes incremental_index
backend/src/indexer/mod.rs      # visibility/signature tweaks only (no consolidation)
backend/tests/                  # new warm-start integration test
```

## Acceptance Criteria (Pass/Fail)

- [ ] Startup indexing skips SHA-256 hashing and ffprobe for files whose size+mtime are unchanged (skip count observable in logs/stats)
- [ ] New files are indexed, modified files are re-hashed and updated, deleted files are removed
- [ ] Phase 2 (Tantivy reindex on read-only connection) still runs after Phase 1 on startup
- [ ] Progress events still broadcast during startup indexing (SSE `indexing_*` unchanged)
- [ ] A manual config change (PUT /config) still triggers the same startup-equivalent indexing run as before
- [ ] `cargo test` green; existing indexer integration tests pass unchanged

## Implementation Notes

- Replace the `full_index` call in the startup pipeline (`main.rs:320`) with `incremental_index`; keep the call signature and progress plumbing identical.
- Log a startup line summarizing `added / updated / skipped / removed` so the skip behavior is verifiable in real runs.
- The skip gate compares stored `file_size` + `file_modified_at` against disk stat — verify the watcher keeps both columns fresh (it does via `store_file`), otherwise warm starts after watcher events could produce stale skips.
- Leave `full_index` public and tested; it remains the PUT /config-triggered path until 8.13 unifies them (review leaves this choice open — if 8.13 decides otherwise, only wiring changes).

## Test Strategy

TDD: write the failing warm-start test first.

```rust
// backend/tests/startup_incremental_index.rs (TestApp + tempdir folder)
#[tokio::test]
async fn warm_start_skips_unchanged_files() {
    // 1. Create folder with 3 files, run startup index → 3 indexed
    // 2. Mutate: modify file A (content + mtime), add file B, delete file C
    // 3. Run startup index again
    // → stats: skipped == 2, added == 1, updated == 1, removed == 1
    // → DB rows reflect the mutations
}
```

Manual verification from the review checklist: restart against a large existing library and confirm startup does **not** re-hash unchanged files (log shows skips).
