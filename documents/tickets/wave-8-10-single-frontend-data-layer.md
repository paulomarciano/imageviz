# Wave 8.10 — Single Data Layer for Grid and Detail View (Jotai Atom)

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 10 |
| **Estimate** | 2 hours |
| **Depends on** | — |
| **Parallel** | Yes (frontend; independent of backend tickets) |
| **Source** | Code review §2 D7 (🟡) / §3 P8-adjacent correctness |

---

## Overview

`ActiveViewContent` (`frontend/src/App.tsx:26-67`) calls `useInfiniteMedia`/`useSearch` so the detail view can navigate, while `ThumbnailGrid` (`frontend/src/components/media/thumbnail-grid.tsx:42-125`) internally calls **the same hooks again** — duplicated data-layer code *with drift*: App.tsx hardcodes `useSearch(query, 100, undefined, 'recency')`, ignoring the mime-filter and sort atoms the grid uses.

Consequences: in search mode with a non-default sort or mime filter, **two different search queries execute per keystroke**, and arrow-key navigation in the detail view follows a *different order than the grid displays*.

Fix: lift the fetched data into a Jotai atom written by the grid; `App.tsx` (DetailView) reads it. Delete the duplicate hooks from `ActiveViewContent`.

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §2 D7
- `frontend/src/App.tsx:26-67` — `ActiveViewContent` duplicate hooks
- `frontend/src/components/media/thumbnail-grid.tsx:42-125` — grid's hook usage + conditional mounting
- `frontend/src/hooks/use-infinite-media.ts`, `use-search.ts`
- `frontend/src/test-utils/` — MSW handlers, render helpers
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/atoms/media-data.ts (or state/)   # atom family/derivation holding the active flat item list
frontend/src/components/media/thumbnail-grid.tsx  # writes flattened items to the atom (useAtomValue + effect)
frontend/src/App.tsx                            # ActiveViewContent reads the atom; duplicate hooks deleted
frontend/src/components/media/__tests__/        # nav-order + single-query tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] `App.tsx` no longer calls `useInfiniteMedia` or `useSearch` directly (grep-clean)
- [ ] In search mode, exactly **one** search network request fires per keystroke (MSW request-counter test), regardless of sort/mime-filter state
- [ ] With `sort=score` (non-default) and an active mime filter, detail-view arrow-key order matches grid display order exactly
- [ ] Infinite-scroll page accumulation is reflected in the atom as pages load (detail navigation can cross page boundaries)
- [ ] Atom is reset/cleared when switching modes (browse ↔ search) and on query change, so DetailView never navigates into stale items
- [ ] Existing grid, detail-view, and keyboard-navigation tests pass
- [ ] `npm test`, `npm run typecheck`, `npm run lint` green

## Implementation Notes

- Shape: store the *derived flat list* the grid renders (`items: MediaItemSummary[]`) plus a mode discriminator — DetailView only needs order + ids, not query state.
- Write from the grid with a `useEffect` on the flattened pages (grid already computes this for rendering); avoid storing TanStack Query internals in the atom — keep it serializable data.
- Deriving "which items exist" from the query cache directly (`queryClient.getQueryData`) is an acceptable alternative if the effect-write proves awkward — pick one mechanism, don't do both.
- Conditional view mounting (grid only mounted when active) means the atom writer only exists when a grid is on screen — verify DetailView is always opened *from* a rendered grid so the atom is populated before first navigation.

## Test Strategy

- MSW counter test: type a query with `sort=recency` changed to `score` + mime filter → assert search endpoint hit exactly once per debounced keystroke.
- Component test: render grid with mixed-type fixture and non-default sort → open detail → arrow through → sequence of `media/:id/file` requests matches grid order.
- Stale-atom test: change query while detail open → navigation list updates or closes; never serves items from the previous query.
