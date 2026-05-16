# Wave 4.9 — Add Scroll Position Restoration

| Field | Value |
|-------|-------|
| **Wave** | 4 — Frontend: Core Layout & Infinite Scroll |
| **Seq** | 09 |
| **Estimate** | 1 hour |
| **Depends on** | 4.7 (thumbnail grid) |
| **Parallel** | Can run in parallel with 4.8 |

---

## Overview

Implement scroll position restoration so that when a user opens a detail view and returns to the grid, they land at their previous scroll position rather than the top of the page. This uses session storage to persist the scroll position and a Jotai atom to restore it on grid mount.

## Prerequisites

- Thumbnail grid with react-virtuoso (4.7)
- `jotai` installed (from 0.3)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 4 task 4.9
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/hooks/
├── use-scroll-restore.ts        # Scroll position persist/restore hook
└── __tests__/
    └── use-scroll-restore.test.ts  # Hook tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Scroll position is saved to `sessionStorage` when the user leaves the grid (unmounts or navigates away)
- [ ] Scroll position is restored when the user returns to the grid (on mount)
- [ ] Storage key includes a unique identifier (e.g., `"imageviz-scroll-position"`)
- [ ] Positions are stored per-browser-tab (sessionStorage, not localStorage)
- [ ] If no saved position exists, grid starts at the top (first load)
- [ ] Works with react-virtuoso's imperative scroll method (`scrollerRef.current.scrollTo({ top })`)

## Implementation Notes

**Jotai atom for scroll position:**
```typescript
// frontend/src/store/ui-atoms.ts
import { atomWithStorage } from 'jotai/utils';

// Use sessionStorage — per-tab, cleared on tab close
export const scrollPositionAtom = atomWithStorage<number>('imageviz-scroll-pos', 0);
```

**Hook:**
```typescript
import { useCallback, useEffect, useRef } from 'react';
import { useAtom } from 'jotai';
import { scrollPositionAtom } from '../store/ui-atoms';

export function useScrollRestore() {
  const [savedPosition, setSavedPosition] = useAtom(scrollPositionAtom);
  const scrollerRef = useRef<HTMLDivElement>(null);

  // Restore scroll position on mount
  useEffect(() => {
    if (savedPosition > 0 && scrollerRef.current) {
      // Use requestAnimationFrame to ensure DOM is ready
      requestAnimationFrame(() => {
        scrollerRef.current?.scrollTo({ top: savedPosition, behavior: 'instant' });
      });
    }
  }, []); // Only on mount

  // Save scroll position on scroll events
  const handleScroll = useCallback(() => {
    if (scrollerRef.current) {
      setSavedPosition(scrollerRef.current.scrollTop);
    }
  }, [setSavedPosition]);

  return { scrollerRef, handleScroll };
}
```

**Integration with react-virtuoso:**
```tsx
// In thumbnail-grid.tsx
import { useScrollRestore } from '../../hooks/use-scroll-restore';

export function ThumbnailGrid({ onItemClick }: ThumbnailGridProps) {
  const { scrollerRef, handleScroll } = useScrollRestore();
  
  return (
    <VirtuosoGrid
      scrollerRef={(ref) => {
        // Combine react-virtuoso's ref with our ref
        if (ref instanceof HTMLElement) {
          scrollerRef.current = ref as HTMLDivElement;
          // Restore scroll position now that ref is set
          ref.addEventListener('scroll', handleScroll, { passive: true });
        }
      }}
      // ... other props
    />
  );
}
```

**Alternative — use react-virtuoso's `initialTopMostItemIndex`:** A more precise approach for virtualized lists:
```typescript
// Save the index of the first visible item, not the pixel position
// (More robust — pixel positions can shift when new items load)
export const gridIndexAtom = atomWithStorage<number>('imageviz-grid-index', 0);

// In grid:
<VirtuosoGrid
  initialTopMostItemIndex={savedIndex}
  rangeChanged={({ startIndex }) => setSavedIndex(startIndex)}
/>
```

This is more robust because item indices are stable, while pixel positions can change when above-the-fold items load lazily.

**Recommendation:** Use the index-based approach since react-virtuoso supports `initialTopMostItemIndex`. Save the `startIndex` from the `rangeChanged` callback.

## Test Strategy

```typescript
import { renderHook, act } from '@testing-library/react';
import { useScrollRestore } from '../use-scroll-restore';

describe('useScrollRestore', () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  it('returns ref and scroll handler', () => {
    const { result } = renderHook(() => useScrollRestore());
    expect(result.current.scrollerRef).toBeDefined();
    expect(result.current.handleScroll).toBeDefined();
  });

  it('saves scroll position on scroll', () => {
    const { result } = renderHook(() => useScrollRestore());
    
    const mockElement = { scrollTop: 500 } as HTMLDivElement;
    act(() => {
      result.current.scrollerRef.current = mockElement;
      result.current.handleScroll();
    });
    
    expect(sessionStorage.getItem('imageviz-grid-index')).toBeTruthy();
  });
});
```
