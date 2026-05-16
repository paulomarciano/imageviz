# Wave 5.2 — Implement Search → Grid Wiring (Jotai Atoms)

| Field | Value |
|-------|-------|
| **Wave** | 5 — Frontend: Search, Detail View & Drag-and-Drop |
| **Seq** | 02 |
| **Estimate** | 1.5 hours |
| **Depends on** | 5.1 (search bar), 4.7 (thumbnail grid) |
| **Parallel** | No |

---

## Overview

Wire the search bar to the thumbnail grid using Jotai atoms. When the user types a search query, the grid switches from the full media list to search results. When the search is cleared, the grid returns to the full list. This is the state management glue connecting the search bar and the grid.

## Prerequisites

- Search bar component (5.1)
- Thumbnail grid (4.7)
- `useSearch` hook (4.4)
- `jotai` installed (from 0.3)

## Reference Files

- `documents/plans/development-plan.md` — §12 project structure (search-atoms.ts, media-atoms.ts)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/store/
├── search-atoms.ts              # Search query atom
├── media-atoms.ts               # Media view mode atom (list vs search)
└── (ui-atoms.ts — already exists from 4.9, or create separately)
```

## Acceptance Criteria (Pass/Fail)

- [ ] `searchQueryAtom` (Jotai atom) holds the current search query string
- [ ] When `searchQueryAtom` is non-empty: grid uses `useSearch(query)` for results
- [ ] When `searchQueryAtom` is empty: grid uses `useInfiniteMedia()` for full list
- [ ] `mediaViewModeAtom` derives from `searchQueryAtom`: `'browse' | 'search'`
- [ ] Grid component reads `mediaViewModeAtom` to decide which data source to use
- [ ] Search results count displayed (e.g., "42 results for 'sunset'")
- [ ] Changing search query instantly updates the grid (debounced by the search hook)
- [ ] Clearing search returns to full list at scroll position (preserved via 4.9)

## Implementation Notes

**search-atoms.ts:**
```typescript
import { atom } from 'jotai';

export const searchQueryAtom = atom<string>('');

// Derived atom: view mode based on whether there's a search query
export const mediaViewModeAtom = atom<'browse' | 'search'>((get) => {
  const query = get(searchQueryAtom);
  return query.trim().length > 0 ? 'search' : 'browse';
});
```

**media-atoms.ts:**
```typescript
import { atom } from 'jotai';
import type { MediaItem } from '../types/media';

// Optional: selected item for detail view
export const selectedMediaItemAtom = atom<MediaItem | null>(null);

// Optional: detail view open/closed
export const detailViewOpenAtom = atom<boolean>(false);
```

**Updated grid component:**
```tsx
import { useAtomValue } from 'jotai';
import { searchQueryAtom, mediaViewModeAtom } from '../../store/search-atoms';
import { useSearch } from '../../hooks/use-search';

export function ThumbnailGrid({ onItemClick }: ThumbnailGridProps) {
  const searchQuery = useAtomValue(searchQueryAtom);
  const viewMode = useAtomValue(mediaViewModeAtom);
  
  // Choose data source based on view mode
  const browseData = useInfiniteMedia();
  const searchData = useSearch(searchQuery);
  
  const activeData = viewMode === 'search' ? searchData : browseData;
  
  const {
    allItems,     // or results from searchData
    totalCount,
    isLoading,
    isError,
    error,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    refetch,
  } = activeData;

  // Use a unified interface. For search data, rename `results` to `allItems`
  const items = viewMode === 'search' 
    ? searchData.results 
    : browseData.allItems;

  // ... render VirtuosoGrid with `items`
}
```

**Search results count (in the grid header or above the grid):**
```tsx
{viewMode === 'search' && (
  <div className="px-3 pt-2 text-sm text-gray-400">
    {searchData.totalCount} result{searchData.totalCount !== 1 ? 's' : ''} for "{searchQuery}"
    <button 
      onClick={() => setSearchQuery('')} 
      className="ml-2 text-blue-400 hover:text-blue-300"
    >
      Clear
    </button>
  </div>
)}
```

**State flow:**
```
User types in SearchBar
  → localQuery state updates
  → After 300ms debounce, searchQueryAtom updates
  → mediaViewModeAtom recomputes to 'search'
  → ThumbnailGrid re-renders with useSearch(query)
  → Search results appear in grid

User clears search (Escape or × button)
  → searchQueryAtom set to ''
  → mediaViewModeAtom recomputes to 'browse'
  → ThumbnailGrid re-renders with useInfiniteMedia()
  → Full list appears at saved scroll position
```

## Test Strategy

```typescript
import { renderHook } from '@testing-library/react';
import { useAtomValue, useSetAtom } from 'jotai';
import { searchQueryAtom, mediaViewModeAtom } from '../search-atoms';

describe('search atoms', () => {
  it('mediaViewMode is browse when query is empty', () => {
    const { result } = renderHook(() => useAtomValue(mediaViewModeAtom));
    expect(result.current).toBe('browse');
  });

  it('mediaViewMode is search when query is non-empty', async () => {
    // Need a wrapper with Provider
    const store = createStore();
    store.set(searchQueryAtom, 'sunset');
    
    const { result } = renderHook(
      () => useAtomValue(mediaViewModeAtom),
      { wrapper: ({ children }) => <Provider store={store}>{children}</Provider> }
    );
    
    expect(result.current).toBe('search');
  });
});
```
