# Wave 4.3 — Implement `useInfiniteMedia` Hook (TanStack Query)

| Field | Value |
|-------|-------|
| **Wave** | 4 — Frontend: Core Layout & Infinite Scroll |
| **Seq** | 03 |
| **Estimate** | 2 hours |
| **Depends on** | 4.2 (API client) |
| **Parallel** | No |

---

## Overview

Implement the `useInfiniteMedia` hook using TanStack Query's `useInfiniteQuery`. This hook manages cursor-based pagination for the media list — fetching pages, tracking loading state, and providing `fetchNextPage` for infinite scroll.

## Prerequisites

- API client (4.2)
- TanStack Query installed and configured (QueryClientProvider in App)
- API types (4.1)

## Reference Files

- `documents/plans/development-plan.md` — §2 Tech Stack (TanStack Query useInfiniteQuery), §3.4 cursor pagination, §8.3 Memory Management (maxPages: 10)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/hooks/
└── use-infinite-media.ts        # useInfiniteMedia hook
└── __tests__/
    └── use-infinite-media.test.ts  # Hook tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Hook returns `{ data, fetchNextPage, hasNextPage, isFetchingNextPage, isLoading, isError, error }`
- [ ] `data.pages` is an array of `PaginatedResponse<MediaItem>` pages
- [ ] `fetchNextPage()` fetches the next cursor page automatically
- [ ] `hasNextPage` is `true` when `meta.has_more` is true, `false` otherwise
- [ ] First page fetched on mount (no manual trigger needed)
- [ ] `maxPages: 10` configured to limit memory (only last 10 pages = 1000 items)
- [ ] Cursor parameters (`cursor`, `cursor_id`) passed from `meta.next_cursor` / `meta.next_cursor_id`
- [ ] Stale time: 5 minutes (don't refetch on every focus change)
- [ ] Error state properly propagated
- [ ] Unit test: hook returns mock data pages

## Implementation Notes

```typescript
import { useInfiniteQuery } from '@tanstack/react-query';
import { fetchMediaList } from '../api/media';
import type { MediaItem, PaginatedResponse } from '../types';

export function useInfiniteMedia(limit = 100) {
  return useInfiniteQuery<PaginatedResponse<MediaItem>, Error>({
    queryKey: ['media', 'list', { limit }],
    queryFn: ({ pageParam }) => {
      const cursor = pageParam as { cursor?: string; cursor_id?: string } | undefined;
      return fetchMediaList({
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
    staleTime: 5 * 60 * 1000,      // 5 minutes
    gcTime: 30 * 60 * 1000,         // 30 minutes garbage collection
    refetchOnWindowFocus: false,
    maxPages: 10,                    // Only keep last 10 pages in memory
  });
}
```

**Return type expansion for convenience:**
```typescript
export function useInfiniteMedia(limit = 100) {
  const query = useInfiniteQuery({ /* ... */ });
  
  // Flatten all pages into a single array for the grid
  const allItems: MediaItem[] = query.data?.pages.flatMap(page => page.data) ?? [];
  const totalCount = query.data?.pages[0]?.meta.total ?? 0;
  
  return {
    ...query,
    allItems,
    totalCount,
    isEmpty: !query.isLoading && allItems.length === 0,
  };
}
```

**QueryClient provider setup** (done in `App.tsx` or `main.tsx`):
```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 2,
      refetchOnWindowFocus: false,
    },
  },
});

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      {/* app content */}
    </QueryClientProvider>
  );
}
```

## Test Strategy

```typescript
// frontend/src/hooks/__tests__/use-infinite-media.test.ts
import { describe, it, expect, vi } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useInfiniteMedia } from '../use-infinite-media';
import * as mediaApi from '../../api/media';

vi.mock('../../api/media');

function wrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

describe('useInfiniteMedia', () => {
  it('fetches first page on mount', async () => {
    const mockPage: PaginatedResponse<MediaItem> = {
      data: [{ id: '1', filename: 'test.png', /* ... */ }],
      meta: { next_cursor: null, next_cursor_id: null, has_more: false, total: 1 },
    };
    vi.mocked(mediaApi.fetchMediaList).mockResolvedValue(mockPage);

    const { result } = renderHook(() => useInfiniteMedia(), { wrapper });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.allItems).toHaveLength(1);
    expect(result.current.totalCount).toBe(1);
    expect(result.current.hasNextPage).toBe(false);
  });

  it('has next page when meta.has_more is true', async () => {
    vi.mocked(mediaApi.fetchMediaList).mockResolvedValue({
      data: [],
      meta: { next_cursor: '2025-01-01T00:00:00Z', next_cursor_id: 'uuid', has_more: true, total: 100 },
    });

    const { result } = renderHook(() => useInfiniteMedia(), { wrapper });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.hasNextPage).toBe(true);
  });
});
```

## External Docs

Use **ExternalScout** to fetch current TanStack Query v5 docs for:
- `useInfiniteQuery` API — `initialPageParam`, `getNextPageParam`, `maxPages`
- Query options: `staleTime`, `gcTime` (renamed from `cacheTime` in v5)
