# Wave 8.21 — Backend KISS & Dead-Code Sweep (K2, K4, K5, K6)

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 21 |
| **Estimate** | 1.5 hours |
| **Depends on** | 8.13 (K6's `incremental_index` item is resolved there — don't delete it here first) |
| **Parallel** | No |
| **Source** | Code review §1 K2 (🟡), K4 (🟡), K5 (🟡), K6 (🔵, backend items) |

---

## Overview

A batch of small, independent backend simplifications:

| Item | Location | Action |
|------|----------|--------|
| **K2** Unreachable failure mode | `search/mod.rs:29,102,115,149,170` | `Mutex<Option<IndexWriter>>` → `Mutex<IndexWriter>`; the `"IndexWriter has been consumed"` error is unreachable (nothing ever `.take()`s) |
| **K4** Redundant `unsafe impl Send/Sync` | `db/pool.rs:77-78` | `SqliteConnectionManager` holds only `Option<PathBuf>` + `bool` — compiler already derives it; delete both blocks + safety comment |
| **K5** In-memory test pool trap | `db/pool.rs:97-101` | Each pooled connection to `sqlite::memory:` is a **separate empty DB**; two concurrent `pool.get()`s see different data. Use `max_size(1)` (or `sqlite::memory:?cache=shared`) for the in-memory variant |
| **K6** Dead code (backend) | see below | `IndexManager::refresh()` (alias, 0 callers); `compute_file_hash_blocking()` (`scanner/hasher.rs:43-45`, only its test); `extract_png_metadata()` wrapper (`metadata/detect.rs:67-69`, pass-through, only tests); `Json(json!(row))` double serialization (`routes/media/detail.rs:82` → `Json(row-with-meta)` directly) |

K6's `incremental_index` and `es.onmessage` items are handled by 8.13 and 8.23 respectively.

## Prerequisites

- 8.13 merged (so the dead-code sweep doesn't delete something 8.13 is about to consume)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §1 K2, K4, K5, K6
- `backend/src/search/mod.rs`, `backend/src/db/pool.rs`, `backend/src/scanner/hasher.rs`, `backend/src/metadata/detect.rs`, `backend/src/routes/media/detail.rs`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/search/mod.rs              # Mutex<IndexWriter>; refresh() deleted
backend/src/db/pool.rs                 # unsafe impls deleted; in-memory pool max_size(1)
backend/src/scanner/hasher.rs          # compute_file_hash_blocking deleted (test inlined/removed)
backend/src/metadata/detect.rs         # extract_png_metadata wrapper deleted (tests call detect_media path)
backend/src/routes/media/detail.rs     # single serialization
```

## Acceptance Criteria (Pass/Fail)

- [ ] `IndexManager` stores `Mutex<IndexWriter>`; no `ok_or`/`Option` unwrap chain remains; the unreachable error string is gone
- [ ] `grep -n "unsafe" backend/src/db/pool.rs` returns nothing
- [ ] A test that holds **two** connections from the in-memory pool observes the same data (guards the K5 fix forever)
- [ ] All K6 symbols deleted; `cargo build` warns on nothing; `grep` finds no callers left behind
- [ ] `detail.rs` returns `Json` of the struct (with meta wrapper) — response bytes identical (integration test unchanged)
- [ ] `cargo test` green, `cargo clippy -- -D warnings` green, `cargo fmt --check` green

## Implementation Notes

- K2: keep the struct's external API identical (`commit`, `add_document`, …) — callers shouldn't change.
- K5: `max_size(1)` is the KISS choice; `cache=shared` changes locking semantics (file-lock contention) and is unnecessary for tests. Note the pool built for production file DBs is untouched.
- K6 `detail.rs`: if the meta wrapper is built as `json!({"meta": …, "item": row})`, construct it once — don't serialize `row` to a `Value` first.

## Test Strategy

- K5 regression test (the two-connection invariant) is the TDD anchor — write it first, watch it fail on current code, then fix.
- Existing search suite covers K2 behavior (commit/write paths).
- Detail-route integration test proves byte-identical response.

## Resolution Notes (2026-09-12 execution)

- K5 criterion 3 ("a test that **holds two** connections observes the same data")
  is unsatisfiable under this ticket's own `max_size(1)` fix. Implemented test
  (`db/pool_test.rs`) instead pins single-connection-ness: with the sole
  connection held, a second `get_timeout` must fail — strictly stronger, and
  also rejects a future `cache=shared` + larger pool.
- detail.rs: response bytes are semantically identical but not byte-identical —
  `json!` sorted keys (BTreeMap) vs struct declaration order. `Value`-parsing
  integration tests unchanged and green; key order is insignificant JSON.
