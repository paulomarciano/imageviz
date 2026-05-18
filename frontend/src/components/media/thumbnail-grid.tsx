import { forwardRef, useCallback, useState, useEffect, type HTMLAttributes } from 'react';
import { VirtuosoGrid } from 'react-virtuoso';
import { useAtomValue } from 'jotai';
import {
  searchQueryAtom,
  mediaViewModeAtom,
  mediaTypeFilterAtom,
  searchSortAtom,
  mimeTypePattern,
} from '../../store/search-atoms';
import { useInfiniteMedia } from '../../hooks/use-infinite-media';
import { useSearch } from '../../hooks/use-search';
import { useScrollRestore } from '../../hooks/use-scroll-restore';
import { useKeyboardNav } from '../../hooks/use-keyboard-nav';
import { ThumbnailCard } from './thumbnail-card';
import { DragSource } from './drag-source';
import { EmptyState } from '../shared/empty-state';
import { ErrorState } from '../shared/error-state';
import { SkeletonGrid } from './skeleton-grid';
import type { MediaItem } from '../../types/media';

interface ThumbnailGridProps {
  readonly onItemClick: (item: MediaItem) => void;
}

const ListContainer = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>((props, ref) => (
  <div
    ref={ref}
    {...props}
    className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3 p-3"
  />
));

const ItemContainer = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>((props, ref) => (
  <div ref={ref} {...props} className="w-full" />
));

/**
 * Browse mode inner component — only mounts useInfiniteMedia.
 * Fully unmounted when switching to search mode, so no wasted query observer.
 */
function BrowseGrid({ onItemClick }: ThumbnailGridProps) {
  const mediaTypeFilter = useAtomValue(mediaTypeFilterAtom);
  const mimeType = mimeTypePattern(mediaTypeFilter);

  const {
    allItems,
    isLoading,
    isError,
    error,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    refetch,
    totalCount,
  } = useInfiniteMedia(100, mimeType, true);

  return (
    <MediaGrid
      items={allItems}
      totalCount={totalCount}
      isLoading={isLoading}
      isError={isError}
      error={error}
      fetchNextPage={fetchNextPage}
      hasNextPage={hasNextPage}
      isFetchingNextPage={isFetchingNextPage}
      refetch={refetch}
      onItemClick={onItemClick}
      searchQuery=""
      searchTotal={0}
    />
  );
}

/**
 * Search mode inner component — only mounts useSearch.
 * Fully unmounted when switching to browse mode, so no wasted query observer.
 */
function SearchGrid({ onItemClick }: ThumbnailGridProps) {
  const searchQuery = useAtomValue(searchQueryAtom);
  const mediaTypeFilter = useAtomValue(mediaTypeFilterAtom);
  const mimeType = mimeTypePattern(mediaTypeFilter);
  const sort = useAtomValue(searchSortAtom);

  const {
    results,
    totalCount,
    isLoading,
    isError,
    error,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    refetch,
    noResults,
  } = useSearch(searchQuery, 100, mimeType, sort);

  // Search-specific: show no-results state before the grid
  if (noResults) {
    return (
      <div className="p-6">
        <p className="text-gray-400 text-sm mb-1">0 results for &ldquo;{searchQuery}&rdquo;</p>
        <EmptyState message="No media matches your search. Try different keywords." />
      </div>
    );
  }

  return (
    <MediaGrid
      items={results}
      totalCount={totalCount}
      isLoading={isLoading}
      isError={isError}
      error={error}
      fetchNextPage={fetchNextPage}
      hasNextPage={hasNextPage}
      isFetchingNextPage={isFetchingNextPage}
      refetch={refetch}
      onItemClick={onItemClick}
      searchQuery={searchQuery}
      searchTotal={totalCount}
    />
  );
}

/** Shared grid rendering logic used by both browse and search modes. */
function MediaGrid({
  items,
  totalCount,
  isLoading,
  isError,
  error,
  fetchNextPage,
  hasNextPage,
  isFetchingNextPage,
  refetch,
  onItemClick,
  searchQuery,
  searchTotal,
}: {
  items: readonly MediaItem[];
  totalCount: number;
  isLoading: boolean;
  isError: boolean;
  error: Error | null;
  fetchNextPage: () => void;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  refetch: () => void;
  onItemClick: (item: MediaItem) => void;
  searchQuery: string;
  searchTotal: number;
}) {
  const loadMore = useCallback(() => {
    if (hasNextPage && !isFetchingNextPage) {
      fetchNextPage();
    }
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  const { savedIndex, handleRangeChanged } = useScrollRestore();

  // Determine number of columns from grid class for keyboard nav.
  // Updates on window resize so keyboard nav stays accurate.
  const computeColumns = useCallback(() => {
    if (typeof window !== 'undefined') {
      if (window.innerWidth >= 1280) return 5;
      if (window.innerWidth >= 1024) return 4;
      if (window.innerWidth >= 640) return 3;
    }
    return 2;
  }, []);

  const [columns, setColumns] = useState(computeColumns);

  useEffect(() => {
    const onResize = () => setColumns(computeColumns());
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [computeColumns]);

  const { focusIndex, containerRef, handleKeyDown } = useKeyboardNav({
    itemCount: items.length,
    columns,
    onSelect: () => {},
    onOpen: (index) => {
      const item = items[index];
      if (item) onItemClick(item);
    },
  });

  const itemContent = useCallback(
    (index: number) => {
      const item = items[index];
      if (!item) return null;
      return (
        <DragSource item={item}>
          <ThumbnailCard
            item={item}
            index={index}
            isFocused={focusIndex === index}
            onClick={onItemClick}
          />
        </DragSource>
      );
    },
    [items, focusIndex, onItemClick],
  );

  if (isLoading) {
    return <SkeletonGrid />;
  }

  if (isError) {
    return (
      <ErrorState message={error?.message ?? 'Failed to load media'} onRetry={() => refetch()} />
    );
  }

  if (items.length === 0) {
    return (
      <EmptyState message="No media found. Configure watched folders in Settings to start browsing." />
    );
  }

  const inSearchMode = !!searchQuery;

  return (
    <div
      className="h-full relative"
      ref={containerRef}
      onKeyDown={handleKeyDown}
      role="grid"
      aria-label="Media gallery"
      aria-busy={isFetchingNextPage}
    >
      {/* Search results count */}
      {inSearchMode && (
        <div className="px-3 pt-2 pb-1 text-sm text-gray-400">
          {searchTotal > 0
            ? `${searchTotal} result${searchTotal !== 1 ? 's' : ''} for "${searchQuery}"`
            : `Searching...`}
        </div>
      )}

      {/* Screen reader live region */}
      <div aria-live="polite" aria-atomic="true" className="sr-only">
        {inSearchMode
          ? `${searchTotal} result${searchTotal !== 1 ? 's' : ''} for "${searchQuery}"`
          : `Showing ${items.length} of ${totalCount} media items`}
      </div>

      <VirtuosoGrid
        style={{ height: '100%' }}
        totalCount={items.length}
        components={{
          List: ListContainer,
          Item: ItemContainer,
        }}
        itemContent={itemContent}
        endReached={loadMore}
        overscan={200}
        increaseViewportBy={200}
        computeItemKey={(index) => items[index]?.id ?? index}
        initialTopMostItemIndex={savedIndex}
        rangeChanged={handleRangeChanged}
      />
      {isFetchingNextPage && (
        <div className="absolute bottom-0 left-0 right-0 flex justify-center py-4 bg-gradient-to-t from-gray-900">
          <div className="animate-spin h-6 w-6 border-2 border-blue-500 border-t-transparent rounded-full" />
        </div>
      )}
    </div>
  );
}

export function ThumbnailGrid({ onItemClick }: ThumbnailGridProps) {
  const viewMode = useAtomValue(mediaViewModeAtom);

  // Conditionally render only the active mode's component so that the
  // unused hook (useInfiniteMedia or useSearch) is fully unmounted.
  // This eliminates wasted TanStack Query observer bookkeeping.
  if (viewMode === 'search') {
    return <SearchGrid onItemClick={onItemClick} />;
  }
  return <BrowseGrid onItemClick={onItemClick} />;
}
