# Wave 8.29 — Serialize Index Runs Against Config Updates (Folder Resurrection Race)

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 29 |
| **Estimate** | 1 hour |
| **Depends on** | 8.04 (single watched-folder source of truth) |
| **Parallel** | Yes |
| **Source** | Code review follow-up on wave-8-04 (`687ba69`/`3926756`) — reviewer warning W-2 |

---

## Overview

`PUT /api/v1/config` spawns a background `full_index` that receives a **clone of the
handler's config** (`routes/config.rs` re-index spawn → `indexer::full_index(&pool,
&config_clone, …)`). `full_index` starts by calling `assign_folder_ids`, which is
upsert-only: it re-inserts rows for every folder in *its snapshot*.

If a second PUT removes folder A while the first PUT's index run is between its
config load and its `assign_folder_ids` commit, that run re-inserts A's row (id
preserved), scans A on disk, and re-inserts its media — **the user's removal is
silently reverted**. A reappears in `GET /config` and after restart. The window is
narrow (sub-second overlap of two quick PUTs) but real, and the failure converges
only because a *later* index run's `remove_deleted_items` cleans up.

Pre-existing in shape (the handler always passed a snapshot), made user-reachable by
8.04's removal semantics.

## Prerequisites

- Wave 8.04 merged.

## Reference Files

- `backend/src/routes/config.rs` — `update_config` spawn block
- `backend/src/indexer/mod.rs` — `full_index` / `incremental_index` (config clone +
  `assign_folder_ids` at run start), `remove_deleted_items`
- `backend/src/indexer/progress.rs` — `ProgressTracker` (possible serialization point)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/indexer/mod.rs           # index runs load config from the table at start
backend/src/routes/config.rs         # pass no config clone (or only as a fallback)
```

## Acceptance Criteria (Pass/Fail)

- [ ] A `full_index` run started by PUT #1 no longer resurrects a folder removed by
      PUT #2 that lands while the run is in flight
- [ ] Index runs read the watched-folder set from the `watched_folders` table at run
      start (single source of truth) instead of trusting a handler-owned snapshot
- [ ] Two rapid PUTs cannot run overlapping index passes on stale folder sets
      (serialize runs or supersede: latest config wins)
- [ ] Regression test: seed folder A with media → PUT removing A → immediately PUT
      adding B → after the spawned indexing settles, `GET /config` shows only B and
      A's rows (folder + media) are gone
- [ ] `cargo test` green, `cargo clippy -- -D warnings` clean, `cargo fmt --check` clean

## Implementation Notes

- Preferred shape: `full_index(pool, progress)` loads `crate::config::load_config`
  itself; route handlers stop passing a snapshot. Keeps one config source and kills
  the race by construction.
- Alternative: serialize index runs behind the existing `ProgressTracker` state or a
  dedicated `tokio::sync::Mutex<()>` — latest PUT's run waits for the previous one,
  then reads the (now current) table.
- Keep `incremental_index` consistent with whichever shape is chosen (both entry
  points must agree).

## Test Strategy

- Integration test per acceptance criterion (in-memory pool, real router).
- Unit test: `full_index` picks up a folder added to the table *after* the run was
  spawned (snapshot-vs-table behavioral proof).
