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

function SkeletonGrid() {
  return (
    <div
      className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3 p-3"
      role="grid"
      aria-label="Loading media"
      aria-busy="true"
    >
      {Array.from({ length: 20 }, (_, i) => (
        <div key={i} className="aspect-[3/4] bg-gray-800 rounded-lg animate-pulse" />
      ))}
    </div>
  );
}

export function ThumbnailGrid({ onItemClick }: ThumbnailGridProps) {
  const searchQuery = useAtomValue(searchQueryAtom);
  const viewMode = useAtomValue(mediaViewModeAtom);
  const mediaTypeFilter = useAtomValue(mediaTypeFilterAtom);
  const mimeType = mimeTypePattern(mediaTypeFilter);
  const sort = useAtomValue(searchSortAtom);

  const browseData = useInfiniteMedia(100, mimeType);
  const searchData = useSearch(searchQuery, 100, mimeType, sort);

  const {
    allItems: browseItems,
    isLoading: browseLoading,
    isError: browseIsError,
    error: browseError,
    fetchNextPage: browseFetchNext,
    hasNextPage: browseHasNext,
    isFetchingNextPage: browseFetchingNext,
    refetch: browseRefetch,
  } = browseData;

  const {
    results: searchResults,
    totalCount: searchTotal,
    isLoading: searchLoading,
    isError: searchIsError,
    error: searchError,
    fetchNextPage: searchFetchNext,
    hasNextPage: searchHasNext,
    isFetchingNextPage: searchFetchingNext,
    refetch: searchRefetch,
    noResults,
  } = searchData;

  const items = viewMode === 'search' ? searchResults : browseItems;
  const isLoading = viewMode === 'search' ? searchLoading : browseLoading;
  const isError = viewMode === 'search' ? searchIsError : browseIsError;
  const error = viewMode === 'search' ? searchError : browseError;
  const fetchNextPage = viewMode === 'search' ? searchFetchNext : browseFetchNext;
  const hasNextPage = viewMode === 'search' ? searchHasNext : browseHasNext;
  const isFetchingNextPage = viewMode === 'search' ? searchFetchingNext : browseFetchingNext;
  const refetch = viewMode === 'search' ? searchRefetch : browseRefetch;

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
    onSelect: () => {}, // Future multi-select
    onOpen: (index) => {
      const item = items[index];
      if (item) onItemClick(item);
    },
  });

  if (isLoading) {
    return <SkeletonGrid />;
  }

  if (isError) {
    return (
      <ErrorState message={error?.message ?? 'Failed to load media'} onRetry={() => refetch()} />
    );
  }

  if (viewMode === 'search' && noResults) {
    return (
      <div className="p-6">
        <p className="text-gray-400 text-sm mb-1">0 results for &ldquo;{searchQuery}&rdquo;</p>
        <EmptyState message="No media matches your search. Try different keywords." />
      </div>
    );
  }

  if (items.length === 0) {
    return (
      <EmptyState message="No media found. Configure watched folders in Settings to start browsing." />
    );
  }

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
      {viewMode === 'search' && (
        <div className="px-3 pt-2 pb-1 text-sm text-gray-400">
          {searchTotal > 0
            ? `${searchTotal} result${searchTotal !== 1 ? 's' : ''} for "${searchQuery}"`
            : `Searching...`}
        </div>
      )}

      {/* Screen reader live region */}
      <div aria-live="polite" aria-atomic="true" className="sr-only">
        {viewMode === 'search'
          ? `${searchTotal} result${searchTotal !== 1 ? 's' : ''} for "${searchQuery}"`
          : `Showing ${items.length} of ${browseData.totalCount} media items`}
      </div>

      <VirtuosoGrid
        style={{ height: '100%' }}
        totalCount={items.length}
        components={{
          List: ListContainer,
          Item: ItemContainer,
        }}
        itemContent={(index) => {
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
        }}
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
