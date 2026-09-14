# Wave 8.15 — Shared PNG-Metadata Serialization and Timestamp Formatting

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 15 |
| **Estimate** | 45 minutes |
| **Depends on** | — |
| **Parallel** | Yes |
| **Source** | Code review §2 D4 + D5 (🟡) |

---

## Overview

Two byte-level duplications in the backend:

1. **PNG metadata → JSON** (D4): the condition `prompt.is_some() || workflow.is_some() || !raw_text_entries.is_empty()` plus identical serialization exists in `backend/src/indexer/mod.rs:290-302` and `backend/src/watcher/stages/extract.rs:50-61` — with a comment in `extract.rs` saying it "must match the indexer's logic". A comment that exists *because* the code is duplicated.
2. **Timestamp → ISO 8601** (D5): `datetime_to_iso` (`scanner/walker.rs:95-101`) and `system_time_to_iso` (`watcher/handler.rs:264-269`) are byte-for-byte equivalent.

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §2 D4, D5
- `backend/src/indexer/mod.rs:290-302`, `backend/src/watcher/stages/extract.rs:50-61`
- `backend/src/scanner/walker.rs:95-101`, `backend/src/watcher/handler.rs:264-269`
- `backend/src/metadata/png.rs`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/metadata/png.rs        # pub fn metadata_to_json(meta: &Metadata) -> Option<String>
backend/src/media_types.rs (or backend/src/util.rs)  # pub fn system_time_to_iso(t: SystemTime) -> String
backend/src/indexer/mod.rs         # calls metadata_to_json
backend/src/watcher/stages/extract.rs  # calls metadata_to_json; "must match" comment deleted
backend/src/scanner/walker.rs      # calls shared formatter
backend/src/watcher/handler.rs     # calls shared formatter; local copy deleted
```

## Acceptance Criteria (Pass/Fail)

- [ ] `metadata_to_json` is the only place deciding "has content → serialized JSON" for PNG metadata; both call sites use it
- [ ] The `"must match the indexer's logic"` comment is gone (nothing left to match)
- [ ] Exactly one timestamp→ISO formatter exists; both `walker` and watcher `handler` call it
- [ ] Serialization output byte-identical to current behavior (golden test pins it — SSE consumers and the metadata panel depend on the shape)
- [ ] `cargo test` green

## Implementation Notes

- Home for the formatter: prefer a small `backend/src/util.rs` if more shared helpers appear (8.x wave adds a couple), otherwise `media_types.rs`. Pick one; do not create both.
- Keep `Metadata`'s serde derives as-is; `metadata_to_json` owns only the *emptiness gate* + `serde_json::to_string`.
- Note for 8.23: the same "shared util" pattern is applied on the frontend there — no coordination needed.

## Test Strategy

- Move one existing test as the golden test: `metadata_to_json` for (prompt only), (workflow only), (raw entries only), (all empty → `None`), (combined) — assert exact JSON strings.
- Timestamp: property-style sample test over a few known `SystemTime` values asserting the same output the old functions produced (copy expected strings before deleting them).
