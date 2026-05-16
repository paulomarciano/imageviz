# Wave 4.7 — Build Virtualized Thumbnail Grid (react-virtuoso)

| Field | Value |
|-------|-------|
| **Wave** | 4 — Frontend: Core Layout & Infinite Scroll |
| **Seq** | 07 |
| **Estimate** | 3 hours |
| **Depends on** | 4.3 (useInfiniteMedia hook), 4.6 (thumbnail card) |
| **Parallel** | No |

---

## Overview

Build the virtualized thumbnail grid using `react-virtuoso` with infinite scroll. The grid renders only visible items (plus a small buffer) in the DOM, enabling smooth performance with 100K+ items. As the user scrolls, `fetchNextPage` loads additional pages via cursor pagination.

## Prerequisites

- `useInfiniteMedia` hook (4.3)
- `ThumbnailCard` component (4.6)
- `react-virtuoso` installed (from 0.3)

## Reference Files

- `documents/plans/development-plan.md` — §2 Tech Stack (react-virtuoso), §8.1 Performance Targets (60fps scroll), §8.2 Virtual scrolling rationale, §9 Risk Register (variable-height items)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/media/
├── thumbnail-grid.tsx           # Virtual scroll grid
└── __tests__/
    └── thumbnail-grid.test.tsx  # Component tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Uses `react-virtuoso` `<VirtuosoGrid>` component with custom item rendering
- [ ] Each item is a `ThumbnailCard` (from 4.6)
- [ ] Infinite scroll: `endReached` callback triggers `fetchNextPage()`
- [ ] Shows loading indicator at the bottom while fetching next page
- [ ] Shows empty state when no media items are indexed ("No media found. Configure watched folders...")
- [ ] Shows initial loading skeleton while first page loads
- [ ] Shows error state with retry button when API fails
- [ ] Grid adapts to container width (CSS grid layout)
- [ ] Smooth scrolling at 60fps with thousands of items
- [ ] Click handler on each card → opens detail view (wired in Wave 5.6)
- [ ] Item sizes are estimated (variable height handled by react-virtuoso's `itemSize` estimation or `useWindowScroll`)
- [ ] Memoized to prevent unnecessary re-renders

## Implementation Notes

```tsx
import { useCallback } from 'react';
import { VirtuosoGrid } from 'react-virtuoso';
import { useInfiniteMedia } from '../../hooks/use-infinite-media';
import { ThumbnailCard } from './thumbnail-card';
import { EmptyState } from '../shared/empty-state';
import { ErrorState } from '../shared/error-state';
import type { MediaItem } from '../../types/media';

interface ThumbnailGridProps {
  onItemClick: (item: MediaItem) => void;
}

export function ThumbnailGrid({ onItemClick }: ThumbnailGridProps) {
  const {
    allItems,
    totalCount,
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

  // Loading state (first page)
  if (isLoading) {
    return <SkeletonGrid />;
  }

  // Error state
  if (isError) {
    return <ErrorState message={error?.message ?? 'Failed to load media'} onRetry={() => refetch()} />;
  }

  // Empty state
  if (allItems.length === 0) {
    return <EmptyState message="No media found. Configure watched folders in Settings to start browsing." />;
  }

  return (
    <VirtuosoGrid
      style={{ height: '100%' }}
      totalCount={allItems.length}
      components={{
        List: ListContainer,
        Item: ItemContainer,
      }}
      itemContent={(index) => (
        <ThumbnailCard
          item={allItems[index]}
          onClick={onItemClick}
        />
      )}
      endReached={loadMore}
      overscan={200} // Pre-render 200px ahead for smoother scrolling
      increaseViewportBy={200}
      computeItemKey={(index) => allItems[index]?.id ?? index}
    />
  );
}

// Custom container components for CSS Grid layout
function ListContainer({ children, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      {...props}
      className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3 p-3"
    >
      {children}
    </div>
  );
}

function ItemContainer({ children, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div {...props} className="w-full">
      {children}
    </div>
  );
}
```

**Grid column strategy:**
- 2 columns on very small screens (rare for desktop)
- 3 columns on vertical 1080p (default — per §10.Q1, 3-4 images per row)
- 4 columns on standard horizontal
- 5 columns on wide/ultrawide

This is achieved via Tailwind responsive classes: `grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5`.

**Overscan:** `overscan={200}` means react-virtuoso renders items 200px above and below the visible viewport. This prevents blank areas during fast scrolling.

**`computeItemKey`:** Essential for correct DOM recycling — uses the item's UUID as the stable key, so React doesn't remount cards when they scroll in/out of view.

**Loading indicator at bottom:**
```tsx
{isFetchingNextPage && (
  <div className="col-span-full flex justify-center py-4">
    <div className="animate-spin h-6 w-6 border-2 border-blue-500 border-t-transparent rounded-full" />
  </div>
)}
```

## Test Strategy

```tsx
// Mock react-virtuoso — it's complex to test in jsdom
vi.mock('react-virtuoso', () => ({
  VirtuosoGrid: vi.fn(({ itemContent, totalCount, components }) => {
    const items = Array.from({ length: totalCount }, (_, i) => itemContent(i));
    const ListComp = components?.List ?? 'div';
    const ItemComp = components?.Item ?? 'div';
    return (
      <ListComp>
        {items.map((item, i) => <ItemComp key={i}>{item}</ItemComp>)}
      </ListComp>
    );
  }),
}));

describe('ThumbnailGrid', () => {
  it('renders thumbnail cards for each item', async () => {
    vi.mocked(useInfiniteMedia).mockReturnValue({
      allItems: [mockItem, mockItem2],
      isLoading: false,
      isError: false,
      // ...
    });

    render(<ThumbnailGrid onItemClick={vi.fn()} />);
    expect(screen.getByText(mockItem.filename)).toBeInTheDocument();
  });

  it('shows loading skeleton on first load', () => {
    vi.mocked(useInfiniteMedia).mockReturnValue({ isLoading: true, /* ... */ });
    render(<ThumbnailGrid onItemClick={vi.fn()} />);
    // Verify skeleton elements present
  });

  it('calls onItemClick when card clicked', async () => { /* ... */ });
});
```

## External Docs

Use **ExternalScout** to fetch current react-virtuoso v4 docs for:
- `<VirtuosoGrid>` — props (`totalCount`, `itemContent`, `endReached`, `overscan`, `computeItemKey`)
- Custom container components (`components.List`, `components.Item`)
- Performance tuning for variable-height items
