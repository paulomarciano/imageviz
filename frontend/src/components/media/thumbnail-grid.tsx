import { forwardRef, useCallback, type HTMLAttributes } from 'react';
import { VirtuosoGrid } from 'react-virtuoso';
import { useInfiniteMedia } from '../../hooks/use-infinite-media';
import { ThumbnailCard } from './thumbnail-card';
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
    <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3 p-3">
      {Array.from({ length: 20 }, (_, i) => (
        <div key={i} className="aspect-[3/4] bg-gray-800 rounded-lg animate-pulse" />
      ))}
    </div>
  );
}

export function ThumbnailGrid({ onItemClick }: ThumbnailGridProps) {
  const {
    allItems,
    isLoading,
    isError,
    error,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    refetch,
  } = useInfiniteMedia();

  const loadMore = useCallback(() => {
    if (hasNextPage && !isFetchingNextPage) {
      fetchNextPage();
    }
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  if (isLoading) {
    return <SkeletonGrid />;
  }

  if (isError) {
    return (
      <ErrorState message={error?.message ?? 'Failed to load media'} onRetry={() => refetch()} />
    );
  }

  if (allItems.length === 0) {
    return (
      <EmptyState message="No media found. Configure watched folders in Settings to start browsing." />
    );
  }

  return (
    <div className="h-full relative">
      <VirtuosoGrid
        style={{ height: '100%' }}
        totalCount={allItems.length}
        components={{
          List: ListContainer,
          Item: ItemContainer,
        }}
        itemContent={(index) => {
          const item = allItems[index];
          if (!item) return null;
          return <ThumbnailCard item={item} onClick={onItemClick} />;
        }}
        endReached={loadMore}
        overscan={200}
        increaseViewportBy={200}
        computeItemKey={(index) => allItems[index]?.id ?? index}
      />
      {isFetchingNextPage && (
        <div className="absolute bottom-0 left-0 right-0 flex justify-center py-4 bg-gradient-to-t from-gray-900">
          <div className="animate-spin h-6 w-6 border-2 border-blue-500 border-t-transparent rounded-full" />
        </div>
      )}
    </div>
  );
}
