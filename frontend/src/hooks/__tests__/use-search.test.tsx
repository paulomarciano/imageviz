/**
 * @vitest-environment jsdom
 *
 * Tests for useSearch — verifies debounce behavior, empty-query gating,
 * result fetching, and convenience booleans.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useSearch } from '../use-search';
import * as searchApi from '../../api/search';
import {
  createCursorPage,
  createCursorPages,
  createMockMediaItem,
} from '../../test-utils/infinite-query-mocks';
import type { PaginatedResponse, MediaItem } from '../../types';

// Mock the search API module.
vi.mock('../../api/search');

/** Build a QueryClient wrapper that disables retry for deterministic tests. */
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

/** Shared mock response for non-empty queries. */
const mockSearchResult: PaginatedResponse<MediaItem> = createCursorPage({
  data: [createMockMediaItem({ filename: 'result.png', path: '2025/result.png' })],
  total: 1,
  query: 'test',
});

describe('useSearch', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('does not search when query is empty', async () => {
    // Arrange
    vi.mocked(searchApi.searchMedia).mockResolvedValue({
      data: [],
      meta: {
        next_cursor: null,
        next_cursor_id: null,
        has_more: false,
        total: 0,
      },
    });

    // Act
    const { result } = renderHook(() => useSearch(''), {
      wrapper: createWrapper(),
    });

    // Assert — the hook should not call searchMedia for empty queries.
    // The query is disabled (enabled = false) so no network call is made.
    expect(searchApi.searchMedia).not.toHaveBeenCalled();
    expect(result.current.isLoading).toBe(false);
  });

  it('returns results for non-empty query after debounce', async () => {
    // Arrange
    vi.mocked(searchApi.searchMedia).mockResolvedValue(mockSearchResult);

    // Act
    const { result } = renderHook(() => useSearch('test'), {
      wrapper: createWrapper(),
    });

    // Assert — wait for the 300ms debounce + query resolution
    await waitFor(() => expect(result.current.isSuccess).toBe(true), {
      timeout: 1000,
    });
    expect(result.current.results).toHaveLength(1);
    expect(result.current.totalCount).toBe(1);
    expect(result.current.hasResults).toBe(true);
    expect(result.current.noResults).toBe(false);
  });

  it('disables search when query is empty', () => {
    // Arrange
    vi.mocked(searchApi.searchMedia).mockResolvedValue({
      data: [],
      meta: {
        next_cursor: null,
        next_cursor_id: null,
        has_more: false,
        total: 0,
      },
    });

    // Act
    const { result } = renderHook(() => useSearch(''), {
      wrapper: createWrapper(),
    });

    // Assert
    expect(result.current.isFetching).toBe(false);
  });

  it('retains all fetched result pages (no maxPages eviction)', async () => {
    // Arrange — 12 pages of 1 result each; a maxPages=10 config would evict
    // the oldest 2 pages and permanently drop results 1-2 from the cache.
    const totalPages = 12;
    for (const page of createCursorPages(totalPages, { query: 'test' })) {
      vi.mocked(searchApi.searchMedia).mockResolvedValueOnce(page);
    }

    // Act
    const { result } = renderHook(() => useSearch('test', 1), {
      wrapper: createWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true), {
      timeout: 1000,
    });

    // fetchNextPage is intentionally NOT awaited: awaiting consumes the
    // microtask in which the hook would re-render, leaving result.current
    // stale. waitFor handles the propagation instead.
    for (let i = 1; i < totalPages; i++) {
      result.current.fetchNextPage();
      await waitFor(() => expect(result.current.results).toHaveLength(i + 1));
    }

    // Assert — every page is still in the cache (no oldest-page eviction).
    expect(result.current.results).toHaveLength(totalPages);
    expect(result.current.results.map((item) => item.id)).toEqual(
      Array.from({ length: totalPages }, (_, i) => String(i + 1)),
    );
  });
});
