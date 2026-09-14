# Wave 8.19 — Stats Endpoint: Single Scan + Event-Driven Frontend Refresh

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 19 |
| **Estimate** | 1.5 hours |
| **Depends on** | — |
| **Parallel** | Yes (backend part independent; 8.23 touches the same panel later) |
| **Source** | Code review §3 P7 (🔵) + §4 R4 (🟡) |

---

## Overview

Two halves of one problem:

1. **Backend (P7)**: `GET /stats` (`backend/src/routes/stats.rs:50-91`) runs `COUNT(*)`, `SUM(file_size)`, `GROUP BY mime_type`, and `MAX(indexed_at)` as **four separate passes** over `media_items`.
2. **Frontend (R4)**: while the settings panel is open, `config-panel.tsx:67-71` polls `/stats` every **5 seconds** — 4 full scans every 5s on a 1M-row database, for a panel the user glances at.

Fix: consolidate the SQL (COUNT + SUM + MAX fold into one statement; the mime GROUP BY stays as a second statement — 4 passes → 2), and drive the frontend from the existing SSE stream (`indexing_complete` is already broadcast) with a slow fallback poll only while indexing is active.

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §3 P7, §4 R4
- `backend/src/routes/stats.rs:50-91`
- `frontend/src/components/config/config-panel.tsx:67-71` — 5s interval
- `frontend/src/hooks/use-sse.ts`, `use-sse-grid-updates.ts` — existing event plumbing
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/routes/stats.rs                  # 2 statements instead of 4
frontend/src/components/config/config-panel.tsx  # SSE-driven refresh + conditional slow poll
frontend/src/components/config/__tests__/    # polling behavior tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Handler executes at most **2** statements over `media_items` (combined totals + mime GROUP BY); response JSON shape unchanged
- [ ] Stats refresh on `indexing_complete` SSE event — no timer needed to reflect a finished index run
- [ ] No 5s polling when indexing is idle (component test with fake timers: zero requests over 60s idle)
- [ ] While indexing is active, poll interval is 15–30s (config const), so live numbers still advance between SSE events
- [ ] Opening/closing the panel repeatedly does not leak intervals or SSE subscriptions
- [ ] `cargo test` green; `npm test`, `npm run typecheck`, `npm run lint` green

## Implementation Notes

```sql
-- statement 1 (totals + freshness)
SELECT COUNT(*), COALESCE(SUM(file_size),0), COALESCE(MAX(indexed_at),'') FROM media_items;
-- statement 2 (breakdown)
SELECT mime_type, COUNT(*), COALESCE(SUM(file_size),0) FROM media_items GROUP BY mime_type;
```

- Frontend: subscribe via the existing `use-sse` hook for `indexing_complete`; keep a `refetchInterval` that is `active ? 20_000 : false` keyed off the indexing-status atom/SSE state.
- Do not change the `/stats` API contract — the frontend types stay as-is.

## Test Strategy

- Backend: integration test asserting response fields equal the old four-query implementation on a seeded DB (golden values).
- Frontend: fake-timer test — idle → no fetches; simulated `indexing_started` → fetch every ~20s; `indexing_complete` → one immediate fetch, timer stops.
