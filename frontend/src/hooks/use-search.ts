import { useState, useEffect, useRef } from 'react';
import { useInfiniteQuery } from '@tanstack/react-query';
import { searchMedia } from '../api/search.ts';
import type { MediaItem, PaginatedResponse } from '../types';

/**
 * Custom hook for debounced full-text search with cursor-based pagination.
 *
 * Returns flattened results, total count, pagination controls, and convenience
 * booleans (hasResults, noResults, isDebouncing).
 *
 * Uses TanStack Query's infinite queries with cursor-based page params that
 * match the backend pagination model.
 *
 * @param query - The raw (non-debounced) search query from the user input.
 * @param limit - Max items per page (default 100).
 */
export function useSearch(query: string, limit = 100) {
  const [debouncedQuery, setDebouncedQuery] = useState(query);
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  // Debounce: update debouncedQuery 300ms after the user stops typing.
  useEffect(() => {
    timerRef.current = setTimeout(() => {
      setDebouncedQuery(query);
    }, 300);

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [query]);

  // Disable search when the debounced query is empty.
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
    // Initial page has no cursor — backend returns the first page.
    initialPageParam: undefined as { cursor?: string; cursor_id?: string } | undefined,
    getNextPageParam: (lastPage) => {
      if (!lastPage.meta.has_more) return undefined;
      return {
        cursor: lastPage.meta.next_cursor ?? undefined,
        cursor_id: lastPage.meta.next_cursor_id ?? undefined,
      };
    },
    enabled,
    // Search results change less often than the media list.
    staleTime: 2 * 60 * 1000, // 2 minutes
    gcTime: 10 * 60 * 1000, // 10 minutes
    maxPages: 5, // Keep at most 5 pages in memory.
  });

  // Flatten pages into a single results array for convenience.
  const allResults: MediaItem[] = infiniteQuery.data?.pages.flatMap((page) => page.data) ?? [];
  const totalCount = infiniteQuery.data?.pages[0]?.meta.total ?? 0;

  return {
    ...infiniteQuery,
    results: allResults,
    totalCount,
    /** True while the user is still typing (before the debounce settles). */
    isDebouncing: query !== debouncedQuery,
    /** True when results exist and loading is complete. */
    hasResults: enabled && !infiniteQuery.isLoading && allResults.length > 0,
    /** True when the search completed with zero results. */
    noResults: enabled && !infiniteQuery.isLoading && allResults.length === 0,
  };
}
