# Wave 6.10 — Performance Profiling and Optimization

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 10 |
| **Estimate** | 2 hours |
| **Depends on** | All frontend components (Waves 4–6) |
| **Parallel** | Yes — can run in parallel with 6.9 |

---

## Overview

Profile the frontend for performance bottlenecks and apply optimizations. Focus on scroll performance (60fps target), memory usage (<500MB with 100K items), search responsiveness (<200ms), and initial load time (<1s).

## Prerequisites

- All UI components implemented (Waves 4–6)
- Dev environment with React DevTools and Chrome Performance tab

## Reference Files

- `documents/plans/development-plan.md` — §8.1 Performance Budget, §8.2 Key Performance Decisions, §8.3 Memory Management
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

Performance optimizations applied to existing components. No new files expected.

## Acceptance Criteria (Pass/Fail)

- [ ] Grid scroll maintains 60fps with 10K+ items rendered (measured in Chrome DevTools Performance)
- [ ] Memory usage < 500MB with 100K items in TanStack Query cache (measured in Chrome Task Manager)
- [ ] Search input → grid update latency < 200ms (including 300ms debounce)
- [ ] Initial page load (first 100 thumbnails) < 1 second
- [ ] No unnecessary re-renders during scroll (verified via React DevTools Profiler)
- [ ] Image loading doesn't block scroll (thumbnails use `loading="lazy"`)
- [ ] No layout shifts during image loading (images have fixed aspect ratio containers)
- [ ] TanStack Query `maxPages: 10` configured
- [ ] `React.memo` on ThumbnailCard (already done in 4.6)
- [ ] `useMemo`/`useCallback` used appropriately (not over-used, not missing)

## Implementation Notes

**Checklist of optimizations to verify/apply:**

1. **React.memo on list items:**
   - `ThumbnailCard` already wrapped in `memo` (4.6) ✓
   - Ensure comparison function isn't needed (props are simple)

2. **Virtual scrolling:**
   - react-virtuoso with `overscan={200}` ✓
   - `computeItemKey` stable (uses item ID) ✓

3. **TanStack Query configuration:**
   ```typescript
   maxPages: 10,           // Only 10 pages (~1000 items) in memory
   staleTime: 5 * 60 * 1000, // Don't refetch on every mount
   gcTime: 30 * 60 * 1000,   // Keep cache for 30 min
   ```

4. **Image optimization:**
   - `loading="lazy"` on all thumbnail images ✓
   - `decoding="async"` on thumbnail images (add if missing)
   - Fixed aspect ratio containers prevent layout shift ✓

5. **Bundle size:**
   - Use `lazy()` + `Suspense` for the detail view module:
   ```typescript
   const DetailView = lazy(() => import('./components/viewer/detail-view'));
   ```
   This prevents the detail view code from loading on initial page load.

6. **Debounce/throttle:**
   - Search input: 300ms debounce ✓ (4.4)
   - Scroll events: react-virtuoso handles this internally ✓

7. **Avoid re-renders:**
   - Jotai atoms — fine-grained subscriptions (component re-renders only when its specific atom changes)
   - TanStack Query — manages its own caching, no extra re-renders
   - Avoid prop drilling that triggers full tree re-renders

8. **Memory leak prevention:**
   - SSE EventSource cleanup on unmount ✓ (6.1)
   - Query cache garbage collection (`gcTime`) ✓
   - Image URL cleanup (no blob URLs created)

**Profiling steps:**
1. Open Chrome DevTools → Performance tab
2. Record while scrolling the grid with 1000+ items
3. Check FPS meter (should stay green at 60fps)
4. Check React DevTools Profiler for unnecessary re-renders
5. Check Memory tab — take heap snapshot, scroll, take another — compare

**Common fixes if performance is poor:**
- Remove inline arrow functions in JSX (use `useCallback`)
- Avoid `useMemo` on trivial computations (it has overhead)
- Ensure react-virtuoso `totalCount` is stable (not changing on every render)
- Check for state loops (atom updates triggering re-renders triggering atom updates)

## Test Strategy

- Manual profiling with Chrome DevTools Performance tab
- Manual memory measurement with Chrome Task Manager
- No automated tests for performance (these are observational checks)
