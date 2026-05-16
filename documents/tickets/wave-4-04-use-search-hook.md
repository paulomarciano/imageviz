# Wave 4.4 — Implement `useSearch` Hook

| Field | Value |
|-------|-------|
| **Wave** | 4 — Frontend: Core Layout & Infinite Scroll |
| **Seq** | 04 |
| **Estimate** | 1.5 hours |
| **Depends on** | 4.2 (API client) |
| **Parallel** | Can run in parallel with 4.3 |

---

## Overview

Implement the `useSearch` hook using TanStack Query. This hook manages search query state with debounced input, fetches search results with cursor pagination, and returns the normalized results for the grid.

## Prerequisites

- API client (4.2)
- TanStack Query configured
- API types (4.1)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Search endpoint, §10.Q5 (free-text search)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/hooks/
├── use-search.ts                # useSearch hook
└── __tests__/
    └── use-search.test.ts       # Hook tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] `useSearch(query)` accepts a search query string
- [ ] Search is **disabled** when query is empty (no API call)
- [ ] Search query is **debounced** by 300ms (avoid requests on every keystroke)
- [ ] Returns search results with pagination support (same pattern as `useInfiniteMedia`)
- [ ] Returns `{ results, totalCount, isLoading, isError, fetchNextPage, hasNextPage }`
- [ ] Clears previous results when query changes (new search = fresh data)
- [ ] Query key includes the search term for proper caching
- [ ] Unit test: debounced search, pagination, empty query disables

## Implementation Notes

```typescript
import { useInfiniteQuery } from '@tanstack/react-query';
import { useState, useEffect, useRef } from 'react';
import { searchMedia } from '../api/search';
import type { MediaItem, PaginatedResponse } from '../types';

export function useSearch(query: string, limit = 100) {
  // Debounce the query
  const [debouncedQuery, setDebouncedQuery] = useState(query);
  const timerRef = useRef<ReturnType<typeof setTimeout>>();

  useEffect(() => {
    timerRef.current = setTimeout(() => {
      setDebouncedQuery(query);
    }, 300);

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [query]);

  const enabled = debouncedQuery.trim().length > 0;

  const infiniteQuery = useInfiniteQuery<PaginatedResponse<MediaItem>, Error>({
    queryKey: ['search', debouncedQuery, { limit }],
    queryFn: ({ pageParam }) => {
      const cursor = pageParam as { cursor?: string; cursor_id?: string } | undefined;
      return searchMedia({
        q: debouncedQuery,
        limit,
        cursor: cursor?.cursor,
        cursor_id: cursor?.cursor_id,
      });
    },
    initialPageParam: undefined as { cursor?: string; cursor_id?: string } | undefined,
    getNextPageParam: (lastPage) => {
      if (!lastPage.meta.has_more) return undefined;
      return {
        cursor: lastPage.meta.next_cursor ?? undefined,
        cursor_id: lastPage.meta.next_cursor_id ?? undefined,
      };
    },
    enabled,
    staleTime: 2 * 60 * 1000,         // 2 minutes (search results change less often)
    gcTime: 10 * 60 * 1000,
    maxPages: 5,                        // Search results — keep fewer pages
  });

  const allResults: MediaItem[] = infiniteQuery.data?.pages.flatMap(page => page.data) ?? [];
  const totalCount = infiniteQuery.data?.pages[0]?.meta.total ?? 0;

  return {
    ...infiniteQuery,
    results: allResults,
    totalCount,
    isDebouncing: query !== debouncedQuery,
    hasResults: enabled && !infiniteQuery.isLoading && allResults.length > 0,
    noResults: enabled && !infiniteQuery.isLoading && allResults.length === 0,
  };
}
```

**Search vs. Media List distinction:**
- `useInfiniteMedia` — for the default grid view, no query
- `useSearch` — when user types in the search bar, replaces the grid with search results
- The consumer (grid component) uses either hook based on the search state from Jotai (Wave 5.2)

**When query changes:**
- `queryKey` changes → TanStack Query automatically refetches
- Old search results are garbage collected after `gcTime` (10 min)

**Debounce:** 300ms is the standard for search inputs. The `isDebouncing` flag allows the UI to show a "waiting" indicator while the user is still typing.

## Test Strategy

```typescript
import { describe, it, expect, vi } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useSearch } from '../use-search';

describe('useSearch', () => {
  it('does not search when query is empty', async () => {
    vi.mocked(searchMedia).mockResolvedValue(mockResponse);

    const { result } = renderHook(() => useSearch(''), { wrapper });

    // No API call should be made
    expect(searchMedia).not.toHaveBeenCalled();
    expect(result.current.isLoading).toBe(false);
  });

  it('returns results for non-empty query', async () => {
    vi.mocked(searchMedia).mockResolvedValue({
      data: [mockItem],
      meta: { has_more: false, total: 1, query: 'test' },
    });

    const { result } = renderHook(() => useSearch('test'), { wrapper });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.results).toHaveLength(1);
  });

  it('supports pagination', async () => {
    // First page has has_more: true
    // fetchNextPage should be available
  });
});
```
