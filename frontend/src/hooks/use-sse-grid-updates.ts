/**
 * Global SSE event → TanStack Query cache bridge.
 *
 * Connects to the backend SSE stream and:
 * - Prepends new items to the cache on `file_created`
 * - Removes items on `file_deleted`
 * - Invalidates individual item queries on `file_modified`
 * - Invalidates all queries on `indexing_complete` and `lagged`
 */

import { useCallback, useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useSetAtom } from 'jotai';
import { useSse } from './use-sse';
import {
  sseStatusAtom,
  recentSseEventsAtom,
  newFileCountAtom,
} from '../store/sse-atoms';
import type { SseEvent, MediaItem } from '../types';

/**
 * Global SSE event → TanStack Query cache bridge hook.
 *
 * This hook connects to the backend SSE stream and:
 * - Prepends new items to the cache on `file_created`
 * - Removes items on `file_deleted`
 * - Invalidates individual item queries on `file_modified`
 * - Invalidates all queries on `indexing_complete` and `lagged`
 */
export function useSseGridUpdates() {
  const queryClient = useQueryClient();
  const setSseStatus = useSetAtom(sseStatusAtom);
  const setRecentEvents = useSetAtom(recentSseEventsAtom);
  const setNewFileCount = useSetAtom(newFileCountAtom);

  const onEvent = useCallback(
    (event: SseEvent) => {
      // Track recent events (last 50 for debugging)
      setRecentEvents((prev) => [event, ...prev].slice(0, 50));

      switch (event.event) {
        case 'file_created': {
          const newItem = event.data as MediaItem;

          // Prepend to cached media list pages using partial key matching
          // (actual keys include limit param: ['media', 'list', { limit }])
          queryClient.setQueriesData(
            { queryKey: ['media', 'list'] },
            (oldData: unknown) => {
              if (!oldData || typeof oldData !== 'object') return oldData;
              const typed = oldData as {
                pages: Array<{
                  data: MediaItem[];
                  meta: { total?: number };
                }>;
              };
              if (!typed.pages || typed.pages.length === 0) return oldData;

              const firstPage = typed.pages[0]!;
              const newPages = [
                {
                  ...firstPage,
                  data: [newItem, ...firstPage.data],
                  meta: {
                    ...firstPage.meta,
                    total: (firstPage.meta?.total ?? 0) + 1,
                  },
                },
                ...typed.pages.slice(1),
              ];

              return { ...typed, pages: newPages };
            },
          );

          // Track new file count for "new files" indicator
          setNewFileCount((prev) => prev + 1);
          break;
        }

        case 'file_deleted': {
          const deletedId = event.data.id;

          // Remove from cached pages (partial key matching)
          queryClient.setQueriesData(
            { queryKey: ['media', 'list'] },
            (oldData: unknown) => {
              if (!oldData || typeof oldData !== 'object') return oldData;
              const typed = oldData as {
                pages: Array<{
                  data: MediaItem[];
                  meta: { total?: number };
                }>;
              };
              if (!typed.pages) return oldData;

              const newPages = typed.pages.map((page) => ({
                ...page,
                data: page.data.filter((item) => item.id !== deletedId),
                meta: {
                  ...page.meta,
                  total: Math.max(0, (page.meta?.total ?? 0) - 1),
                },
              }));

              return { ...typed, pages: newPages };
            },
          );

          // Also remove from single-item cache
          queryClient.removeQueries({
            queryKey: ['media', 'item', deletedId],
          });
          break;
        }

        case 'file_modified': {
          // Invalidate the specific item's detail query
          queryClient.invalidateQueries({
            queryKey: ['media', 'item', event.data.id],
          });
          break;
        }

        case 'indexing_complete':
        case 'lagged': {
          // Full re-fetch — too many changes to patch individually
          queryClient.invalidateQueries({ queryKey: ['media', 'list'] });
          queryClient.invalidateQueries({ queryKey: ['search'] });
          setNewFileCount(0);
          break;
        }
      }
    },
    [queryClient, setRecentEvents, setNewFileCount],
  );

  const { status } = useSse({ onEvent });

  // Sync connection status to Jotai
  useEffect(() => {
    setSseStatus(status);
  }, [status, setSseStatus]);

  // Expose new file reset function
  const resetNewFileCount = useCallback(() => {
    setNewFileCount(0);
  }, [setNewFileCount]);

  return { status, resetNewFileCount };
}
