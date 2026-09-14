# Wave 8.16 — Introduce AppError and Shared Response Types Across Routes

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 16 |
| **Estimate** | 2 hours |
| **Depends on** | 8.11, 8.12, 8.17, 8.19 (route-touching tickets land first so this sweep is the last route edit) |
| **Parallel** | No |
| **Source** | Code review §2 D6 (🟡) |

---

## Overview

Two duplications across the route layer:

1. The `(StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": …})))` tuple is hand-written **~20 times** (grep-confirmed), and the "Failed to acquire database connection" pool-error block opens nearly every handler.
2. `MediaItemSummary` is defined twice with identical fields: `routes/media/list.rs:31-43` and `routes/search.rs:264-276`.

Fix: a small `AppError` enum (or helper fns `internal_error()` / `pool_error()`) with `IntoResponse`, and a shared `routes/response.rs`. Handlers become `?`-propagating instead of `map_err`-noise.

## Prerequisites

- 8.11, 8.12, 8.17, 8.19 merged (their handler edits should not be rebased over this sweep)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §2 D6
- `backend/src/routes/media/list.rs:31-43`, `backend/src/routes/search.rs:264-276` — duplicate structs
- `backend/src/routes/` — all handlers carrying the error tuples
- Axum docs — `IntoResponse` for enum error types
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/routes/error.rs      # AppError enum + IntoResponse impl (+ From<rusqlite::Error>, From<r2d2::Error>)
backend/src/routes/response.rs   # shared MediaItemSummary (and any other repeated response structs)
backend/src/routes/*.rs          # handlers migrated to ?-propagation / helpers
```

## Acceptance Criteria (Pass/Fail)

- [ ] `MediaItemSummary` defined exactly once; list and search routes use the shared type
- [ ] `StatusCode::INTERNAL_SERVER_ERROR` appears in handler code only inside `error.rs` (grep-verified)
- [ ] The pool-acquisition error block exists exactly once (the `From<r2d2::Error>` conversion)
- [ ] Every handler's external behavior is unchanged: same status codes, same JSON error bodies, same success shapes — integration tests pass **without body assertions being loosened**
- [ ] Handlers that legitimately need custom error bodies (input-validation 400s from 7.6) keep them — via a dedicated `AppError::BadRequest` variant or the existing validators
- [ ] `cargo clippy -- -D warnings`, `cargo fmt --check` green

## Implementation Notes

```rust
pub enum AppError {
    Pool(r2d2::Error),
    Db(rusqlite::Error),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self { ... };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}
```

- Handlers return `Result<impl IntoResponse, AppError>`; `pool.get()?` and query `?` replace `map_err` boilerplate.
- Keep the JSON body key `"error"` and current message strings verbatim — the frontend and e2e tests assert them.
- Scope check: do **not** fold 7.6's structured validation errors into `AppError` unless it is a zero-risk mapping; leave them as-is if their shape differs.

## Test Strategy

- The full existing integration suite is the contract — run it untouched.
- Add one test asserting a pool-starvation scenario (if feasible) maps to the same status/body as before.
- Snapshot-style check on one representative error path per variant (message string unchanged).
