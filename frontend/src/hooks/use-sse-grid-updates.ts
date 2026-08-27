/**
 * Global SSE event → TanStack Query cache bridge.
 *
 * Connects to the backend SSE stream and:
 * - Prepends new items to the cache on `file_created`
 * - Removes items on `file_deleted`
 * - Invalidates individual item queries on `file_modified`
 * - Invalidates all queries on `indexing_complete` and `lagged`
 *
 * Performance: cache mutations are debounced at 50ms so rapid bursts
 * (e.g. initial file scan) are batched into a single update cycle.
 */

import { useCallback, useEffect, useRef } from 'react';
import { useQueryClient, type QueryClient } from '@tanstack/react-query';
import { useSetAtom } from 'jotai';
import { useSse } from './use-sse';
import { sseStatusAtom, recentSseEventsAtom, newFileCountAtom } from '../store/sse-atoms';
import type { SseEvent, MediaItem } from '../types';

/** Events older than this are pruned from the debug log. */
const FIVE_MINUTES_MS = 5 * 60 * 1000;

/** Max recent events kept in the atom for debugging. */
const MAX_RECENT_EVENTS = 50;

/** Accumulate SSE mutations for this many ms before flushing to the cache. */
const BATCH_WINDOW_MS = 50;

/**
 * Truncate a cached infinite query to its first page.
 *
 * Invalidating an infinite query refetches every accumulated page
 * sequentially. Truncating first bounds the subsequent refetch to a single
 * page — the v5-recommended pattern since `refetchPage` was removed.
 * Single-page or absent cache entries are left untouched.
 */
function truncateToFirstPage(queryClient: QueryClient, queryKey: readonly unknown[]): void {
  queryClient.setQueriesData({ queryKey }, (oldData: unknown) => {
    if (!oldData || typeof oldData !== 'object') return oldData;
    const typed = oldData as { pages?: unknown[]; pageParams?: unknown[] };
    // Require both arrays: truncating pages without a matching pageParams
    // entry would corrupt pagination state (fetchNextPage derives the next
    // param from the last pageParams entry).
    if (!typed.pages || !typed.pageParams || typed.pages.length <= 1) return oldData;
    return {
      pages: typed.pages.slice(0, 1),
      pageParams: typed.pageParams.slice(0, 1),
    };
  });
}

type TimedEvent = SseEvent & { _timestamp: number };

/** Batched mutations accumulated between flush cycles. */
interface PendingBatch {
  created: MediaItem[];
  deleted: string[];
  modified: string[];
}

/**
 * Global SSE event → TanStack Query cache bridge hook.
 *
 * This hook connects to the backend SSE stream and:
 * - Prepends new items to the cache on `file_created`
 * - Removes items on `file_deleted`
 * - Invalidates individual item queries on `file_modified`
 * - Invalidates all queries on `indexing_complete` and `lagged`
 * - Prunes events older than 5 minutes from the debug store
 * - Batches rapid mutations into single cache updates
 */
export function useSseGridUpdates() {
  const queryClient = useQueryClient();
  const setSseStatus = useSetAtom(sseStatusAtom);
  const setRecentEvents = useSetAtom(recentSseEventsAtom);
  const setNewFileCount = useSetAtom(newFileCountAtom);
  const recentRef = useRef<TimedEvent[]>([]);
  const batchRef = useRef<PendingBatch>({ created: [], deleted: [], modified: [] });
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ---- Flush accumulated mutations to the cache ----
  const flushBatch = useCallback(() => {
    const batch = batchRef.current;
    batchRef.current = { created: [], deleted: [], modified: [] };
    flushTimerRef.current = null;

    if (batch.created.length > 0) {
      queryClient.setQueriesData({ queryKey: ['media', 'list'] }, (oldData: unknown) => {
        if (!oldData || typeof oldData !== 'object') return oldData;
        const typed = oldData as {
          pages: Array<{ data: MediaItem[]; meta: { total?: number } }>;
        };
        if (!typed.pages || typed.pages.length === 0) return oldData;

        const firstPage = typed.pages[0]!;
        const newPages = [
          {
            ...firstPage,
            data: [...batch.created, ...firstPage.data],
            meta: {
              ...firstPage.meta,
              total: (firstPage.meta?.total ?? 0) + batch.created.length,
            },
          },
          ...typed.pages.slice(1),
        ];
        return { ...typed, pages: newPages };
      });
    }

    if (batch.deleted.length > 0) {
      const deletedSet = new Set(batch.deleted);
      queryClient.setQueriesData({ queryKey: ['media', 'list'] }, (oldData: unknown) => {
        if (!oldData || typeof oldData !== 'object') return oldData;
        const typed = oldData as {
          pages: Array<{ data: MediaItem[]; meta: { total?: number } }>;
        };
        if (!typed.pages) return oldData;

        const newPages = typed.pages.map((page) => ({
          ...page,
          data: page.data.filter((item) => !deletedSet.has(item.id)),
          meta: {
            ...page.meta,
            total: Math.max(0, (page.meta?.total ?? 0) - deletedSet.size),
          },
        }));
        return { ...typed, pages: newPages };
      });

      for (const id of batch.deleted) {
        queryClient.removeQueries({ queryKey: ['media', 'item', id] });
      }
    }

    for (const id of batch.modified) {
      queryClient.invalidateQueries({ queryKey: ['media', 'item', id] });
    }
  }, [queryClient]);

  const scheduleFlush = useCallback(() => {
    if (flushTimerRef.current) clearTimeout(flushTimerRef.current);
    flushTimerRef.current = setTimeout(flushBatch, BATCH_WINDOW_MS);
  }, [flushBatch]);

  const onEvent = useCallback(
    (event: SseEvent) => {
      const now = Date.now();
      const timedEvent: TimedEvent = { ...event, _timestamp: now };

      // Track recent events with time-based pruning (for debugging)
      recentRef.current = [timedEvent, ...recentRef.current]
        .filter((e) => now - e._timestamp < FIVE_MINUTES_MS)
        .slice(0, MAX_RECENT_EVENTS);
      setRecentEvents(recentRef.current);

      switch (event.event) {
        case 'file_created': {
          batchRef.current.created.push(event.data as MediaItem);
          scheduleFlush();
          setNewFileCount((prev) => prev + 1);
          break;
        }

        case 'file_deleted': {
          batchRef.current.deleted.push(event.data.id);
          scheduleFlush();
          break;
        }

        case 'file_modified': {
          batchRef.current.modified.push(event.data.id);
          scheduleFlush();
          break;
        }

        case 'indexing_complete':
        case 'lagged': {
          // Flush any pending mutations first, then invalidate.
          if (flushTimerRef.current) {
            clearTimeout(flushTimerRef.current);
            flushBatch();
          }
          // Truncate infinite query caches to the first page BEFORE
          // invalidating, so the refetch fetches one page instead of every
          // accumulated page (a deep-scrolled session could otherwise fire
          // hundreds of sequential requests). The view resets to the top,
          // which is acceptable after a full reindex or a lag event. This
          // includes the search cache: a user deep in search results also
          // resets to the top, even for a reindex unrelated to their query —
          // acceptable since reindexed results may have shifted arbitrarily.
          truncateToFirstPage(queryClient, ['media', 'list']);
          truncateToFirstPage(queryClient, ['search']);
          queryClient.invalidateQueries({ queryKey: ['media', 'list'] });
          queryClient.invalidateQueries({ queryKey: ['search'] });
          setNewFileCount(0);
          break;
        }
      }
    },
    [setRecentEvents, setNewFileCount, scheduleFlush, flushBatch],
  );

  const { status } = useSse({ onEvent });

  // Sync connection status to Jotai
  useEffect(() => {
    setSseStatus(status);
  }, [status, setSseStatus]);

  // Cleanup flush timer on unmount
  useEffect(() => {
    return () => {
      if (flushTimerRef.current) clearTimeout(flushTimerRef.current);
    };
  }, []);

  // Expose new file reset function
  const resetNewFileCount = useCallback(() => {
    setNewFileCount(0);
  }, [setNewFileCount]);

  return { status, resetNewFileCount };
}
