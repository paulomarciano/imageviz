/**
 * @vitest-environment jsdom
 *
 * Tests for useInfiniteMedia — verifies first-page fetch, pagination,
 * page flattening, and empty-state detection.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useInfiniteMedia } from '../use-infinite-media';
import * as mediaApi from '../../api/media';
import type { PaginatedResponse, MediaItem } from '../../types';

// Mock the media API module — all exports become vi.fn() automatically.
vi.mock('../../api/media');

/** Build a QueryClient wrapper that disables retry for deterministic tests. */
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

/** Build a mock paginated response with sensible defaults. */
function createMockPage(
  overrides?: Partial<PaginatedResponse<MediaItem>>,
): PaginatedResponse<MediaItem> {
  return {
    data: [
      {
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
      },
    ],
    meta: {
      next_cursor: null,
      next_cursor_id: null,
      has_more: false,
      total: 1,
    },
    ...overrides,
  };
}

describe('useInfiniteMedia', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('fetches first page on mount', async () => {
    // Arrange
    const mockPage = createMockPage();
    vi.mocked(mediaApi.fetchMediaList).mockResolvedValue(mockPage);

    // Act
    const { result } = renderHook(() => useInfiniteMedia(), {
      wrapper: createWrapper(),
    });

    // Assert
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.allItems).toHaveLength(1);
    expect(result.current.totalCount).toBe(1);
    expect(result.current.isEmpty).toBe(false);
  });

  it('has next page when meta.has_more is true', async () => {
    // Arrange
    vi.mocked(mediaApi.fetchMediaList).mockResolvedValue(
      createMockPage({
        data: [],
        meta: {
          next_cursor: 'cursor',
          next_cursor_id: 'id',
          has_more: true,
          total: 100,
        },
      }),
    );

    // Act
    const { result } = renderHook(() => useInfiniteMedia(), {
      wrapper: createWrapper(),
    });

    // Assert
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.hasNextPage).toBe(true);
  });

  it('flattens multiple pages into allItems', async () => {
    // Arrange
    vi.mocked(mediaApi.fetchMediaList)
      .mockResolvedValueOnce(
        createMockPage({
          data: [{ ...createMockPage().data[0]!, id: '1', filename: 'a.png' }],
          meta: {
            next_cursor: 'c1',
            next_cursor_id: 'id1',
            has_more: true,
            total: 2,
          },
        }),
      )
      .mockResolvedValueOnce(
        createMockPage({
          data: [{ ...createMockPage().data[0]!, id: '2', filename: 'b.png' }],
          meta: {
            next_cursor: null,
            next_cursor_id: null,
            has_more: false,
            total: 2,
          },
        }),
      );

    // Act
    const { result } = renderHook(() => useInfiniteMedia(1), {
      wrapper: createWrapper(),
    });

    // Wait for first page
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    // Fetch second page
    result.current.fetchNextPage();
    await waitFor(() => expect(result.current.allItems).toHaveLength(2));

    // Assert
    expect(result.current.totalCount).toBe(2);
  });

  it('shows empty state when no items', async () => {
    // Arrange
    vi.mocked(mediaApi.fetchMediaList).mockResolvedValue(
      createMockPage({
        data: [],
        meta: {
          next_cursor: null,
          next_cursor_id: null,
          has_more: false,
          total: 0,
        },
      }),
    );

    // Act
    const { result } = renderHook(() => useInfiniteMedia(), {
      wrapper: createWrapper(),
    });

    // Assert
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.isEmpty).toBe(true);
    expect(result.current.allItems).toHaveLength(0);
  });
});
