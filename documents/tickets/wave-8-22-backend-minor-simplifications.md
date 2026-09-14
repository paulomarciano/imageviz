# Wave 8.22 — Backend Minor Simplifications (K7)

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 22 |
| **Estimate** | 1 hour |
| **Depends on** | 8.4 (`routes/config.rs` write path changed), 8.12 + 8.16 (`list.rs` reshaped, error style settled) |
| **Parallel** | No |
| **Source** | Code review §1 K7 (🔵, backend items) |

---

## Overview

Three small backend cleanups:

1. **`routes/media/list.rs:99-135`**: the dynamic SQL builder (`where_parts` + manual `?N` counting + `Vec<&dyn ToSql>` mapping) can become `rusqlite::params_from_iter` over a plain `Vec<Value>`, removing counter bookkeeping and the `param_refs` mapping. Also the outer `let mut items` at line 137 is redundant.
2. **`routes/config.rs:102-117`**: `save_config()` is called explicitly and then again inside `assign_folder_ids()` — one redundant DB write per PUT.
3. **`Cargo.toml`**: `tokio = { features = ["full"] }` pulls in process, signal, io-util, etc. Enumerate only what's used (`rt-multi-thread`, `macros`, `signal`, `fs`, `sync`, `time`, `net`) — smaller binary, faster builds.

## Prerequisites

- 8.4 (config write path simplified first — the double-write may disappear with it)
- 8.12 + 8.16 (`list.rs` count cache and error handling landed — apply on the final shape)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §1 K7
- `backend/src/routes/media/list.rs`, `backend/src/routes/config.rs`, `backend/Cargo.toml`
- `rusqlite` docs — `params_from_iter`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/routes/media/list.rs   # params_from_iter builder; redundant mut removed
backend/src/routes/config.rs       # single save per PUT
backend/Cargo.toml                 # explicit tokio features
```

## Acceptance Criteria (Pass/Fail)

- [ ] List route builds its WHERE clause from a `Vec<serde_json::Value>` (or `Vec<Box<dyn ToSql>>`) with `params_from_iter` — no manual `?N` counter remains
- [ ] All list combinations still return identical results: no filter, mime filter, search-adjacent params, cursor variants (existing 3.4 tests + new combination test)
- [ ] One `save_config`-equivalent write per PUT /config (write-count assertion or code inspection after 8.4)
- [ ] `cargo build` succeeds with enumerated tokio features; `cargo tree -e features -i tokio` shows no missing-feature compile errors across the workspace
- [ ] `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` green

## Implementation Notes

- Feature list to verify against actual usage: `rt-multi-thread`, `macros`, `signal` (graceful shutdown 7.1), `fs` (`tokio::fs`), `sync` (semaphore/watch), `time` (timeouts/intervals), `net` (axum serve). Add `io-util` only if `AsyncReadExt`/`AsyncWriteExt` are used on tokio types (check `ReaderStream` plumbing).
- If a needed feature is missed, the compile error names it — fix by adding, not by reverting to `full`.

## Test Strategy

- The list endpoint's parameterized integration tests are the contract; add one case with filter + cursor + limit together (the combination most likely to break in a builder rewrite).
- PUT /config roundtrip test asserts final state only (write count is an implementation detail — assert via code inspection or an optional SQL trace).
