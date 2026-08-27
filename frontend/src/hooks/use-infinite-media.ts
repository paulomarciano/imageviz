import { useMemo } from 'react';
import { useInfiniteQuery } from '@tanstack/react-query';
import { fetchMediaList } from '../api/media';
import type { MediaItem, PaginatedResponse } from '../types';
import { CursorPageParam, INITIAL_PAGE_PARAM, getNextPageParam } from './use-cursor-pagination';

export function useInfiniteMedia(limit = 100, mimeType?: string, enabled = true) {
  const query = useInfiniteQuery<PaginatedResponse<MediaItem>, Error>({
    queryKey: ['media', 'list', { limit, mimeType: mimeType ?? 'all' }],
    enabled,
    queryFn: ({ pageParam }) => {
      const cursor = pageParam as CursorPageParam;
      return fetchMediaList({
        limit,
        cursor: cursor?.cursor,
        cursor_id: cursor?.cursor_id,
        mime_type: mimeType,
      });
    },
    initialPageParam: INITIAL_PAGE_PARAM,
    getNextPageParam,
    staleTime: 5 * 60 * 1000,
    gcTime: 30 * 60 * 1000,
    refetchOnWindowFocus: false,
  });

  const allItems = useMemo(
    () => query.data?.pages.flatMap((page) => page.data) ?? [],
    [query.data?.pages],
  );
  const totalCount = query.data?.pages[0]?.meta.total ?? 0;

  return {
    ...query,
    allItems,
    totalCount,
    isEmpty: !query.isLoading && allItems.length === 0,
  };
}
