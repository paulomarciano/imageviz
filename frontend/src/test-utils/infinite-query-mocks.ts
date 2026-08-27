/**
 * Shared mock builders for cursor-paginated infinite query tests.
 *
 * Mirrors the backend pagination model (`PaginatedResponse` + cursor meta)
 * so hook tests exercise realistic page shapes.
 */

import type { MediaItem, PaginatedResponse } from '../types';

/** Build a mock MediaItem with sensible defaults. */
export function createMockMediaItem(overrides: Partial<MediaItem> = {}): MediaItem {
  return {
    id: '1',
    filename: 'test.png',
    path: '2025/test.png',
    mime_type: 'image/png',
    thumbnail_url: '/api/v1/media/1/thumbnail',
    width: 896,
    height: 1216,
    file_size: 245_760,
    created_at: '2025-01-01T00:00:00Z',
    modified_at: '2025-01-01T00:00:00Z',
    ...overrides,
  };
}

interface CursorPageOptions {
  /** Items on this page. */
  data: MediaItem[];
  /** Cursor for the next page (null/omitted on the last page). */
  nextCursor?: string | null;
  /** Cursor id for the next page (null/omitted on the last page). */
  nextCursorId?: string | null;
  /** Whether more pages follow (default false). */
  hasMore?: boolean;
  /** Total item count reported in meta (default data.length). */
  total?: number;
  /** Optional search query echoed in meta (search endpoint only). */
  query?: string;
}

/** Build a single cursor-paginated response page. */
export function createCursorPage({
  data,
  nextCursor = null,
  nextCursorId = null,
  hasMore = false,
  total = data.length,
  query,
}: CursorPageOptions): PaginatedResponse<MediaItem> {
  return {
    data,
    meta: {
      next_cursor: nextCursor,
      next_cursor_id: nextCursorId,
      has_more: hasMore,
      total,
      ...(query !== undefined && { query }),
    },
  };
}

/**
 * Build `count` sequential single-item cursor pages with ids '1'..'count'.
 *
 * Useful for retention regression tests: fetching more pages than a
 * `maxPages`-style cap would evict the oldest pages and drop early items.
 */
export function createCursorPages(
  count: number,
  options?: { query?: string },
): PaginatedResponse<MediaItem>[] {
  return Array.from({ length: count }, (_, i) => {
    const isLast = i === count - 1;
    return createCursorPage({
      data: [createMockMediaItem({ id: String(i + 1), filename: `p${i + 1}.png` })],
      nextCursor: isLast ? null : `c${i}`,
      nextCursorId: isLast ? null : `id${i}`,
      hasMore: !isLast,
      total: count,
      query: options?.query,
    });
  });
}
