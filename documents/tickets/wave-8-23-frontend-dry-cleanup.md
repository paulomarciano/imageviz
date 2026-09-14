# Wave 8.23 — Frontend DRY Cleanup (D8 + K6/K7 frontend items)

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 23 |
| **Estimate** | 1.5 hours |
| **Depends on** | 8.10 (App.tsx data layer settled), 8.19 (config-panel polling reshaped) |
| **Parallel** | No |
| **Source** | Code review §2 D8 (🔵) + §1 K6 (`es.onmessage`) + K7 (frontend `put<T>`) |

---

## Overview

Small frontend duplications, some already drifted:

1. **`formatBytes()`** in `config-panel.tsx:9-14` duplicates `formatFileSize()` from `utils/format.ts` — and has drifted: the shared one lacks the GB case the local one has. Extend `formatFileSize` and delete the local copy.
2. **Escape-to-close** handlers independently implemented in `config-panel.tsx:52-58`, `shortcuts-panel.tsx:63-70`, `detail-view.tsx:57-74`. Fold into `useFocusTrap` (add an `onClose` option) or a two-line `useEscape(onClose)` hook.
3. **`saveConfig`** in `config-panel.tsx:24-34` uses raw `fetch` while everything else goes through the typed client — add a `put<T>()` helper next to `get<T>()` in `client.ts` and use it.
4. **Dead `es.onmessage` branch** (`use-sse.ts:61-76`): the backend only sends *named* SSE events, which never trigger `onmessage` — the branch is effectively dead. Delete it.

## Prerequisites

- 8.10 (don't collide with the App.tsx data-layer rewrite)
- 8.19 (config-panel stats wiring changed first)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §2 D8, §1 K6 (frontend item), K7 (frontend item)
- `frontend/src/components/config/config-panel.tsx`, `frontend/src/components/shortcuts/shortcuts-panel.tsx`, `frontend/src/components/detail/detail-view.tsx`
- `frontend/src/utils/format.ts`, `frontend/src/api/client.ts`, `frontend/src/hooks/use-sse.ts`, `use-focus-trap.ts`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/utils/format.ts                  # formatFileSize gains GB (and above?) cases
frontend/src/hooks/use-escape.ts (or focus-trap option)  # single escape handler
frontend/src/api/client.ts                    # put<T>()
frontend/src/components/... (3 panels)        # adopt useEscape; drop local copies
frontend/src/hooks/use-sse.ts                 # onmessage branch deleted
frontend/src/**/__tests__/                    # updated tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] `grep -rn "formatBytes" frontend/src` empty; `formatFileSize` renders GB correctly (unit test: bytes → KB → MB → GB boundaries)
- [ ] Escape closes config panel, shortcuts panel, and detail view — one shared hook; component test per panel
- [ ] `client.put<T>()` exists and `saveConfig` uses the typed client; no raw `fetch` in `config-panel.tsx` (grep-clean)
- [ ] `use-sse.ts` has no `onmessage` assignment; SSE named-event tests still pass
- [ ] `npm test`, `npm run typecheck`, `npm run lint` green

## Implementation Notes

- `useEscape(onClose)`: `useEffect` + `keydown` listener with `Escape` check; if `useFocusTrap` already owns focus scope, an `onClose` option there is equally fine — pick one, don't ship both.
- Keep handler semantics identical: `keydown` on document, no preventDefault regressions (detail view video player also binds keys — verify no double-handling after dedup).
- `put<T>()` mirrors `get<T>()`: JSON body, same error envelope, same base URL/proxy behavior.

## Test Strategy

- Unit: `formatFileSize` boundary table (999 B, 1 KB, 1 MB, 1.5 GB…).
- Component: `userEvent.keyboard('{Escape}')` per panel → `onClose` called once.
- Hook test: `use-sse` dispatches a named event → listener fires; dispatches a default-message event → no crash, no handler (branch gone).
