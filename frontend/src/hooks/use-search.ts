import { useInfiniteQuery } from '@tanstack/react-query';
import { searchMedia } from '../api/search.ts';
import type { MediaItem, PaginatedResponse } from '../types';
import type { SearchSort } from '../store/search-atoms.ts';

/**
 * Custom hook for full-text search with cursor-based pagination.
 *
 * Returns flattened results, total count, pagination controls, and convenience
 * booleans (hasResults, noResults).
 *
 * Debouncing is handled upstream by the SearchBar component so this hook
 * uses the `query` value directly — no additional debounce needed here.
 *
 * Uses TanStack Query's infinite queries with cursor-based page params that
 * match the backend pagination model.
 *
 * @param query - The search query to send to the backend (already debounced upstream).
 * @param limit - Max items per page (default 100).
 * @param mimeType - Optional MIME type filter (e.g. `image/%`, `video/%`).
 * @param sort - Sort order — `"recency"` (newest first) or `"score"` (BM25 relevance).
 */
export function useSearch(
  query: string,
  limit = 100,
  mimeType?: string,
  sort: SearchSort = 'recency',
) {
  // Disable search when the query is empty.
  const enabled = query.trim().length > 0;

  const infiniteQuery = useInfiniteQuery<PaginatedResponse<MediaItem>, Error>({
    queryKey: ['search', query, { limit, mimeType: mimeType ?? 'all', sort }],
    queryFn: ({ pageParam }) => {
      const cursor = pageParam as { cursor?: string; cursor_id?: string } | undefined;
      return searchMedia({
        q: query,
        limit,
        cursor: cursor?.cursor,
        cursor_id: cursor?.cursor_id,
        mime_type: mimeType,
        sort,
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
    /** True when results exist and loading is complete. */
    hasResults: enabled && !infiniteQuery.isLoading && allResults.length > 0,
    /** True when the search completed with zero results. */
    noResults: enabled && !infiniteQuery.isLoading && allResults.length === 0,
  };
}
