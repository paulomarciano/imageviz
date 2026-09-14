# Wave 8.24 — Quiet Release: Single Log Line per Request + Default Runtime Workers

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 24 |
| **Estimate** | 30 minutes |
| **Depends on** | 8.20 (main.rs/runtime wiring settled first) |
| **Parallel** | No |
| **Source** | Code review §4 R6 (🟡) + R7 (🔵) |

---

## Overview

1. **R6**: `LogOnRequest` (`backend/src/middleware/logging.rs:64-68`) prints an extra `→ request` line for every request in addition to the `← response` line. A grid load is ~100 thumbnail + file requests → ~200 log lines per page view for an app meant to be quiet. The request ID in the span isn't returned to clients, so the extra line adds little correlation value.
2. **R7**: `#[tokio::main(flavor = "multi_thread", worker_threads = 4)]` (`main.rs:33`) pins the runtime to 4 workers regardless of the machine — oversubscribing a 2-core laptop, wasting 12 cores on a 16-core desktop. The default (workers = CPU count) is the right adaptive choice, and pairs with 8.2's parallel indexing to actually use the extra cores.

## Prerequisites

- 8.20 (dev-tools gating changes the same runtime setup code)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §4 R6, R7
- `backend/src/middleware/logging.rs:64-68`, `backend/src/main.rs:33`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/middleware/logging.rs   # on_request dropped or demoted to DEBUG
backend/src/main.rs                 # worker_threads attribute removed
```

## Acceptance Criteria (Pass/Fail)

- [ ] At `RUST_LOG=info`, one log line per request (the `← response` line with duration + status)
- [ ] At `RUST_LOG=debug`, the `→ request` line is still available (if demoted) for troubleshooting
- [ ] Runtime worker count equals `std::thread::available_parallelism()` (assert via tokio-console session or a debug log; no hard-coded 4 remains)
- [ ] Request-ID/duration fields on the response line unchanged (7.7 logging tests pass)
- [ ] `cargo test` green

## Implementation Notes

- R6: prefer **dropping** `on_request` (the response line already carries method, path, status, duration, request id). If correlation-before-response matters for streaming responses (SSE/3600s), demote to `DEBUG` instead — choose one and note it in the module doc.
- R7: `#[tokio::main]` with `flavor = "multi_thread"` and no `worker_threads` defaults to available parallelism — that's the whole change.

## Test Strategy

- Existing logging middleware tests adjusted: INFO level renders exactly one line per request.
- Manual: load a 100-thumbnail grid, count backend log lines ≈ number of requests (not 2×).
