import { useInfiniteQuery } from '@tanstack/react-query';
import { fetchMediaList } from '../api/media';
import type { MediaItem, PaginatedResponse } from '../types';

export function useInfiniteMedia(limit = 100, mimeType?: string) {
  const query = useInfiniteQuery<PaginatedResponse<MediaItem>, Error>({
    queryKey: ['media', 'list', { limit, mimeType: mimeType ?? 'all' }],
    queryFn: ({ pageParam }) => {
      const cursor = pageParam as { cursor?: string; cursor_id?: string } | undefined;
      return fetchMediaList({
        limit,
        cursor: cursor?.cursor,
        cursor_id: cursor?.cursor_id,
        mime_type: mimeType,
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
    staleTime: 5 * 60 * 1000,
    gcTime: 30 * 60 * 1000,
    refetchOnWindowFocus: false,
    maxPages: 10,
  });

  const allItems: MediaItem[] = query.data?.pages.flatMap((page) => page.data) ?? [];
  const totalCount = query.data?.pages[0]?.meta.total ?? 0;

  return {
    ...query,
    allItems,
    totalCount,
    isEmpty: !query.isLoading && allItems.length === 0,
  };
}
