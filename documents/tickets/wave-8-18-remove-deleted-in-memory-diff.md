# Wave 8.18 — remove_deleted_items: In-Memory Diff Instead of Per-Row Disk Stats

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 18 |
| **Estimate** | 1 hour |
| **Depends on** | 8.13 (function signature changes inside the consolidated `run_index`) |
| **Parallel** | No |
| **Source** | Code review §3 P8 (🔵) |

---

## Overview

`remove_deleted_items` (`backend/src/indexer/mod.rs:384-422`) ignores the complete on-disk file list the same index run already produced (`all_files`) and instead stats the disk once **per DB row** — 1M `exists()` calls on the target dataset — with per-row `DELETE`s outside a transaction, all synchronous blocking I/O inside the async index run.

Fix: build a `HashSet<(folder_id, relative_path)>` from the scan results, diff the DB rows against it in memory, and wrap the deletes in one transaction. Pass the scanned set into the function instead of re-deriving existence from disk.

## Prerequisites

- 8.13 merged (`run_index` core owns the epilogue call site)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §3 P8
- `backend/src/indexer/mod.rs:384-422`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/indexer/mod.rs        # remove_deleted_items(scanned: &HashSet<(i64, String)>, ...)
backend/src/indexer/mod_test.rs   # diff + transaction tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Zero per-row `exists()`/`stat` calls during cleanup (test observes none)
- [ ] All deletes for an index run execute inside a single transaction
- [ ] Removed-file detection is exactly as before: DB rows not present in the current scan (per folder) are deleted; rows whose folder is no longer configured are deleted (preserve current semantics — verify with existing tests)
- [ ] Memory: the set holds `(i64, String)` pairs for the scan — acceptable at 1M rows (~100–150 MB worst case); if that is a concern, hash the pair into a `u64` set instead (document the choice)
- [ ] `cargo test` green; `cargo test -- --ignored` green with fixtures

## Implementation Notes

- The scan stage (`scan_all_folders` result) already walks every configured folder — thread its output into the epilogue instead of letting `remove_deleted_items` re-walk.
- Keep the signature pure: `(pool, scanned_set) -> Result<usize>`; no disk access at all inside the function. This also makes it trivially unit-testable.
- Single transaction: `unchecked_transaction()` on the connection or batch through the existing batch helpers.

## Test Strategy

- Unit: DB with rows [A, B, C], scanned set {A, C} → B deleted, exactly one transaction (assert via commit count or statement counter), returns deleted count 1.
- Edge cases: empty scanned set (everything deleted), empty DB (no-op), duplicate relative paths across different folders (folder_id distinguishes).
- Fixture integration: delete a file on disk between two index runs → gone from DB and grid.
