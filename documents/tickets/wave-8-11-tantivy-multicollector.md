# Wave 8.11 — Single-Traversal Search via MultiCollector

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 11 |
| **Estimate** | 45 minutes |
| **Depends on** | — |
| **Parallel** | Yes (8.12 touches a different file; 8.16 builds on this) |
| **Source** | Code review §3 P3 (🟡) |

---

## Overview

`backend/src/routes/search.rs:152-200` executes `searcher.search(&query, &Count)` — walking all matching docs to compute the total — and then `searcher.search(&query, &TopDocs…)` — walking them **again**. Tantivy's `MultiCollector` returns count + top-docs in a single traversal, halving the per-keystroke search cost on large indexes.

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §3 P3
- `backend/src/routes/search.rs:152-200` — the two `searcher.search` calls
- `tantivy` crate docs — `MultiCollector`, `MultiCollector::new().with_collector(...)`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/routes/search.rs   # one searcher.search call with MultiCollector
```

## Acceptance Criteria (Pass/Fail)

- [ ] Exactly one `searcher.search(...)` call per query in the route (code-inspection criterion)
- [ ] Response shape identical: `total`, `items[]`, cursor/next-cursor semantics unchanged
- [ ] Existing search integration tests (3.3/3.9) pass **without modification** (behavioral proof)
- [ ] Empty-result, single-result, and many-result (> page size) cases all return correct counts
- [ ] `cargo clippy -- -D warnings` green

## Implementation Notes

```rust
let multicollector = MultiCollector::new()
    .with_collector(top_docs_collector)
    .with_collector(Count);
let (count_guard, mut docs_guard) = searcher.search(&query, &multicollector)?;
let total = count_guard;
let top_docs: Vec<(Score, DocAddress)> = docs_guard.collect();
```

- Keep the existing `TopDocs` collector configuration (limit/offset via `with_limit`/`with_offset`) exactly as-is — only the execution changes.
- Do not touch scoring, query parsing, or snippet/highlight logic in this ticket.

## Test Strategy

- Existing integration tests are the contract (they assert `total` + item order).
- Add one case if missing: fixture index with 25 docs, `limit=10` → `total == 25`, 10 items, correct next cursor (proves count guard reads the full match set, not the page).
