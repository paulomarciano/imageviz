# Wave 8.13 — Consolidate full_index / incremental_index into One run_index Core

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 13 |
| **Estimate** | 1.5 hours |
| **Depends on** | 8.1, 8.2 (indexer/mod.rs edits land first) |
| **Parallel** | No |
| **Source** | Code review §2 D2 (🟡) |

---

## Overview

`full_index` (`backend/src/indexer/mod.rs:47-124`) and `incremental_index` (`136-237`) repeat: the assign-folder-ids block, `scan_all_folders`, the empty-library early return, the `chunks(BATCH_SIZE)` loop with identical `store_file`/stats/error handling, and the `remove_deleted_items` epilogue. The **only** difference is the skip check inside the loop. ~150 duplicated lines that can drift.

Fix: extract one core

```rust
async fn run_index(
    pool: &Pool,
    config: &AppConfig,
    progress: &ProgressHandle,
    skip: impl Fn(&FolderFileEntry, &FolderFileRow) -> bool, // entry vs stored row
) -> Result<IndexStats>
```

`full_index` passes `|_, _| false`; `incremental_index` passes the size+mtime gate. Cuts the file roughly in half and guarantees the two paths cannot diverge.

## Prerequisites

- 8.1 + 8.2 merged (startup wiring + parallel processing — consolidation happens on the final shape, not the old one)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §2 D2
- `backend/src/indexer/mod.rs:47-237` — both functions
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/indexer/mod.rs       # run_index core; full_index/incremental_index become thin wrappers
backend/src/indexer/mod_test.rs  # shared-behavior tests parameterized over both modes
```

## Acceptance Criteria (Pass/Fail)

- [ ] A single `run_index` implementation exists; each wrapper is ≤ ~20 lines
- [ ] `indexer/mod.rs` line count roughly halved vs post-8.2 state
- [ ] Both modes pass the **same** parameterized test suite (full mode: nothing skipped; incremental: unchanged files skipped)
- [ ] Progress events, stats counters, error accounting identical between modes for equivalent inputs
- [ ] Startup (8.1) and PUT /config trigger points unchanged in behavior
- [ ] `cargo test` green; `cargo clippy -- -D warnings` green

## Implementation Notes

- The skip closure receives both the on-disk entry and the stored DB row (the incremental check needs `file_size`/`file_modified_at` from the row). `full_index` ignores both.
- Decide in this ticket what happens to the Tantivy-side `incremental_index` dependency: `search::indexer::incremental_index` is flagged dead in K6 (review §1) — if nothing but its own test calls it after 8.1, **delete it here** and have D3 (8.14) reduce to the `full_reindex` cleanup. Otherwise keep and let 8.14 extract `index_rows`.
- Keep `remove_deleted_items` as the shared epilogue call (its internals are ticket 8.18's concern).

## Test Strategy

- Convert existing per-mode tests into a `#[rstest]`-style parameterized suite (or a plain loop over an enum) so every behavioral test runs against both `full_index` and `incremental_index`.
- Drift guard: assert both functions produce identical DB state when run over a tree where nothing changed (full: all re-hashed; incremental: all skipped; same rows).
