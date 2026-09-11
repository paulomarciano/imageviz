/**
 * @vitest-environment jsdom
 *
 * Tests for useSseGridUpdates — verifies the SSE → TanStack Query cache
 * bridge, focusing on `indexing_complete`/`lagged` invalidation:
 * the cached infinite query must be truncated to its first page before
 * invalidation so the refetch fetches only ONE page instead of every
 * accumulated page (refetchPage was removed in TanStack Query v5).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import {
  QueryClient,
  QueryClientProvider,
  useInfiniteQuery,
  useQuery,
} from '@tanstack/react-query';
import { useSseGridUpdates } from '../use-sse-grid-updates';
import { INITIAL_PAGE_PARAM, getNextPageParam } from '../use-cursor-pagination';
import type { CursorPageParam } from '../use-cursor-pagination';
import { createCursorPages } from '../../test-utils/infinite-query-mocks';
import { MockEventSource } from '../../test-utils/mock-event-source';
import type { PaginatedResponse, MediaItem } from '../../types';

/** Build a QueryClient wrapper that disables retry for deterministic tests. */
function createWrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

describe('useSseGridUpdates', () => {
  beforeEach(() => {
    MockEventSource.reset();
    vi.stubGlobal('EventSource', MockEventSource);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('truncates the media list to the first page on indexing_complete so refetch fetches only one page', async () => {
    // Arrange — a real infinite query with 3 accumulated pages.
    const pages = createCursorPages(3);
    const pageByCursor = new Map<string, PaginatedResponse<MediaItem>>([
      ['c0', pages[1]!],
      ['c1', pages[2]!],
    ]);
    const queryFn = vi.fn((context: { pageParam: unknown }) => {
      const pageParam = context.pageParam as CursorPageParam;
      const page = pageParam === undefined ? pages[0]! : pageByCursor.get(pageParam.cursor!)!;
      return Promise.resolve(page);
    });

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });

    const listHook = renderHook(
      () =>
        useInfiniteQuery<PaginatedResponse<MediaItem>, Error>({
          queryKey: ['media', 'list', { limit: 1, mimeType: 'all' }],
          queryFn,
          initialPageParam: INITIAL_PAGE_PARAM,
          getNextPageParam,
          staleTime: Infinity,
        }),
      { wrapper: createWrapper(queryClient) },
    );
    await waitFor(() => expect(listHook.result.current.isSuccess).toBe(true));
    listHook.result.current.fetchNextPage();
    await waitFor(() => expect(listHook.result.current.data?.pages).toHaveLength(2));
    listHook.result.current.fetchNextPage();
    await waitFor(() => expect(listHook.result.current.data?.pages).toHaveLength(3));
    expect(queryFn).toHaveBeenCalledTimes(3);

    // Mount the SSE bridge against the same QueryClient.
    renderHook(() => useSseGridUpdates(), { wrapper: createWrapper(queryClient) });
    const es = MockEventSource.instances[0]!;

    // Act — trigger indexing_complete.
    act(() => {
      es.triggerEvent('indexing_complete', { total: 3, duration_ms: 100 });
    });

    // Assert — the refetch fetches ONLY the first page (1 extra call, not 3).
    // Assert the settled state: wait for the truncated refetch to complete,
    // then give any (buggy) additional page fetches a chance to fire before
    // pinning the exact call count. A regression would end at 6 calls / 3
    // pages; the correct implementation stays at 4 calls / 1 page.
    await waitFor(() => expect(listHook.result.current.data?.pages).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(queryFn).toHaveBeenCalledTimes(4);
    expect(listHook.result.current.data?.pages[0]!.data[0]!.id).toBe('1');
  });

  it('truncates the media list to the first page on lagged', async () => {
    // Arrange — seed a 3-page cache directly (no active observer needed to
    // verify the truncation itself).
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const pages = createCursorPages(3);
    queryClient.setQueryData(['media', 'list', { limit: 1, mimeType: 'all' }], {
      pages,
      pageParams: [INITIAL_PAGE_PARAM, { cursor: 'c0' }, { cursor: 'c1' }],
    });

    renderHook(() => useSseGridUpdates(), { wrapper: createWrapper(queryClient) });
    const es = MockEventSource.instances[0]!;

    // Act
    act(() => {
      es.triggerEvent('lagged', { skipped: 5 });
    });

    // Assert — cache truncated to the first page and marked invalidated.
    const data = queryClient.getQueryData<{
      pages: PaginatedResponse<MediaItem>[];
      pageParams: unknown[];
    }>(['media', 'list', { limit: 1, mimeType: 'all' }]);
    expect(data?.pages).toHaveLength(1);
    expect(data?.pageParams).toHaveLength(1);
    const state = queryClient.getQueryState(['media', 'list', { limit: 1, mimeType: 'all' }]);
    expect(state?.isInvalidated).toBe(true);
  });

  it('truncates search results to the first page on indexing_complete', async () => {
    // Arrange — seed a 3-page search cache.
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const pages = createCursorPages(3, { query: 'test' });
    queryClient.setQueryData(['search', 'test', { limit: 1, mimeType: 'all', sort: 'recency' }], {
      pages,
      pageParams: [INITIAL_PAGE_PARAM, { cursor: 'c0' }, { cursor: 'c1' }],
    });

    renderHook(() => useSseGridUpdates(), { wrapper: createWrapper(queryClient) });
    const es = MockEventSource.instances[0]!;

    // Act
    act(() => {
      es.triggerEvent('indexing_complete', { total: 3, duration_ms: 100 });
    });

    // Assert
    const data = queryClient.getQueryData<{
      pages: PaginatedResponse<MediaItem>[];
      pageParams: unknown[];
    }>(['search', 'test', { limit: 1, mimeType: 'all', sort: 'recency' }]);
    expect(data?.pages).toHaveLength(1);
    const state = queryClient.getQueryState([
      'search',
      'test',
      { limit: 1, mimeType: 'all', sort: 'recency' },
    ]);
    expect(state?.isInvalidated).toBe(true);
  });

  it('leaves single-page caches untouched (still invalidated)', async () => {
    // Arrange
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const pages = createCursorPages(1);
    queryClient.setQueryData(['media', 'list', { limit: 1, mimeType: 'all' }], {
      pages,
      pageParams: [INITIAL_PAGE_PARAM],
    });

    renderHook(() => useSseGridUpdates(), { wrapper: createWrapper(queryClient) });
    const es = MockEventSource.instances[0]!;

    // Act
    act(() => {
      es.triggerEvent('indexing_complete', { total: 1, duration_ms: 100 });
    });

    // Assert — data preserved, invalidation still applied.
    const data = queryClient.getQueryData<{
      pages: PaginatedResponse<MediaItem>[];
      pageParams: unknown[];
    }>(['media', 'list', { limit: 1, mimeType: 'all' }]);
    expect(data?.pages).toHaveLength(1);
    expect(data?.pages[0]!.data[0]!.id).toBe('1');
    const state = queryClient.getQueryState(['media', 'list', { limit: 1, mimeType: 'all' }]);
    expect(state?.isInvalidated).toBe(true);
  });

  it('invalidates the stats query on indexing_complete so an open panel refetches', async () => {
    // Arrange — a mounted stats observer (e.g. ConfigPanel is open).
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const statsQueryFn = vi.fn(() => Promise.resolve({ total: 3, indexing: { status: 'Idle' } }));
    const stats = renderHook(
      () => useQuery<unknown, Error>({ queryKey: ['stats'], queryFn: statsQueryFn }),
      { wrapper: createWrapper(queryClient) },
    );
    await waitFor(() => expect(stats.result.current.isSuccess).toBe(true));
    expect(statsQueryFn).toHaveBeenCalledTimes(1);

    renderHook(() => useSseGridUpdates(), { wrapper: createWrapper(queryClient) });
    const es = MockEventSource.instances[0]!;

    // Act
    act(() => {
      es.triggerEvent('indexing_complete', { total: 3, duration_ms: 100 });
    });

    // Assert — the observer refetches because the query was invalidated.
    await waitFor(() => expect(statsQueryFn).toHaveBeenCalledTimes(2));
  });
});
