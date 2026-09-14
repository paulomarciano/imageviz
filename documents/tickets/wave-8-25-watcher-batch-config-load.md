# Wave 8.25 — Watcher: Load Watched-Folder Config Once per Event Batch

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 25 |
| **Estimate** | 30 minutes |
| **Depends on** | 8.4 (folder config loading moved to the table — build on the final shape) |
| **Parallel** | No |
| **Source** | Code review §4 R8 (🔵) |

---

## Overview

Each deletion event opens a pooled connection and reloads the watched-folder config (`backend/src/watcher/handler.rs:186-199`). A delete burst of N files performs N identical queries. Folders change rarely — load once per batch in `run_event_handler` and pass the snapshot down.

## Prerequisites

- 8.4 merged (config source is the table; loader signature settled)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §4 R8
- `backend/src/watcher/handler.rs:186-199` — per-event reload
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/watcher/handler.rs   # one config load per batch; threaded through the stage pipeline
```

## Acceptance Criteria (Pass/Fail)

- [ ] A batch of N deletion events performs exactly **one** watched-folder load (query-count test)
- [ ] Behavior identical: deletions inside a watched folder are removed from the index; events outside watched folders are ignored
- [ ] Debounce/batch semantics (500ms) unchanged
- [ ] Existing watcher pipeline tests (3.5/3.6) pass unchanged
- [ ] `cargo test` green

## Implementation Notes

- Keep the snapshot immutable (`Arc<[WatchedFolder]>` or plain `Vec`) and pass by reference through the stages — the pipeline refactor (stages/) already keeps the handler an orchestrator; this stays in that style.
- Do not add cache-invalidation machinery: per-batch reload is the whole fix. A folder added mid-buster being picked up on the next batch is acceptable (and matches pre-existing debounce timing).

## Test Strategy

- Test: enqueue 50 delete events for one folder → handler makes 1 config query (counter via `cfg(test)` helper or SQL trace), all 50 removals processed.
- Test: delete events for a non-watched path in the same batch → ignored, no extra loads.
