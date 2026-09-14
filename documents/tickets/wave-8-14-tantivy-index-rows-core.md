# Wave 8.14 — Extract Shared index_rows Core for Tantivy Indexing

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 14 |
| **Estimate** | 1 hour |
| **Depends on** | 8.13 (settles whether the Tantivy-side incremental entry point survives) |
| **Parallel** | No |
| **Source** | Code review §2 D3 (🟡) |

---

## Overview

`full_reindex` (`backend/src/search/indexer.rs:60-142`) and `incremental_index` (`154-261`) duplicate the 8 field lookups, the row-mapping closure, the `tantivy::doc!` construction, and error accounting — ~80% verbatim copies.

Fix: extract `index_rows(rows, manager)` and let both entry points feed it. **If 8.13 decides to delete the Tantivy-side `incremental_index`** (it is dead code per K6 once startup uses the SQLite-side incremental path), this ticket collapses to: delete the dead function + its test, and simplify `full_reindex` in place. Either outcome resolves D3.

## Prerequisites

- 8.13 merged (its decision determines this ticket's shape)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §2 D3, §1 K6 (dead `incremental_index`)
- `backend/src/search/indexer.rs:60-142, 154-261`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/search/indexer.rs    # index_rows core (or deleted dead path)
backend/src/search/indexer_test.rs
```

## Acceptance Criteria (Pass/Fail)

**Path A (incremental survives):**
- [ ] Field lookups, row mapping, and `doc!` construction exist exactly once in `index_rows`
- [ ] Both entry points produce identical Tantivy documents for identical rows (parameterized test)
- [ ] Error accounting (per-row failure counts) preserved

**Path B (incremental deleted):**
- [ ] `incremental_index` and its test removed; grep finds no callers
- [ ] `full_reindex` behavior unchanged; existing reindex tests pass

- [ ] Either way: `cargo test` green, `cargo clippy -- -D warnings` green

## Implementation Notes

```rust
fn index_rows<'a>(
    writer: &IndexWriter,
    fields: &SearchFields,
    rows: impl Iterator<Item = &FolderFileRow>,
) -> (usize /*indexed*/, usize /*errors*/) { ... }
```

- Keep the batch transaction boundaries of the SQLite side untouched — this ticket only reshapes the Tantivy write loop.
- If Path B: also remove the now-unneeded `pub` visibility and update `search/mod.rs` re-exports.

## Test Strategy

- Path A: parameterized test feeding the same rows through both entry points against a tempdir Tantivy index → identical doc count + field values.
- Path B: existing `full_reindex` tests (3.2) are the contract.
