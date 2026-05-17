import type { PaginatedResponse } from '../types';

/** Cursor-based page parameter for TanStack Query's infinite queries. */
export type CursorPageParam = { cursor?: string; cursor_id?: string } | undefined;

/** Default initial page param — no cursor means "first page". */
export const INITIAL_PAGE_PARAM = undefined as CursorPageParam;

/**
 * Extract the next page cursor from a paginated API response.
 * Returns `undefined` when there are no more pages to fetch.
 */
export function getNextPageParam<T>(lastPage: PaginatedResponse<T>): CursorPageParam {
  if (!lastPage.meta.has_more) return undefined;
  return {
    cursor: lastPage.meta.next_cursor ?? undefined,
    cursor_id: lastPage.meta.next_cursor_id ?? undefined,
  };
}
