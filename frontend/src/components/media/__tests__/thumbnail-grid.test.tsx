/**
 * @vitest-environment jsdom
 *
 * Tests for ThumbnailGrid — verifies responsive grid classes, render states
 * (loading, error, empty, populated), search wiring, and that the responsive
 * layout uses correct Tailwind breakpoints.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ThumbnailGrid } from '../thumbnail-grid';
import type { MediaItem } from '../../../types/media';

/* ------------------------------------------------------------------ */
/*  Hooks & external deps                                             */
/* ------------------------------------------------------------------ */

const mockUseInfiniteMedia = vi.hoisted(() => vi.fn());
const mockUseSearch = vi.hoisted(() => vi.fn());
const mockUseKeyboardNav = vi.hoisted(() => vi.fn());

vi.mock('../../../hooks/use-infinite-media', () => ({
  useInfiniteMedia: mockUseInfiniteMedia,
}));

vi.mock('../../../hooks/use-search', () => ({
  useSearch: mockUseSearch,
}));

vi.mock('../../../hooks/use-keyboard-nav', () => ({
  useKeyboardNav: mockUseKeyboardNav,
}));

/**
 * Mock VirtuosoGrid so we can inspect its List / Item containers
 * without needing a real virtual-scroll environment in jsdom.
 */
vi.mock('react-virtuoso', () => ({
  VirtuosoGrid: ({
    components,
    itemContent,
    totalCount,
  }: {
    components?: {
      List?: React.ComponentType<{ style?: React.CSSProperties; children?: React.ReactNode }>;
      Item?: React.ComponentType<{ style?: React.CSSProperties; children?: React.ReactNode }>;
    };
    itemContent?: (index: number) => React.ReactNode;
    totalCount?: number;
    [key: string]: unknown;
  }) => {
    const List = components?.List ?? 'div';
    const Item = components?.Item ?? 'div';
    const items = Array.from({ length: totalCount ?? 0 }, (_, i) => i);
    return (
      <List>
        {items.map((i) => (
          <Item key={i}>{itemContent?.(i)}</Item>
        ))}
      </List>
    );
  },
}));

/**
 * Mock DragSource to just render children — we don't need DnD in these tests.
 */
vi.mock('../drag-source', () => ({
  DragSource: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

/* ------------------------------------------------------------------ */
/*  Default mock return values                                        */
/* ------------------------------------------------------------------ */

function defaultUseSearchReturn() {
  return {
    results: [],
    totalCount: 0,
    isLoading: false,
    isError: false,
    error: null,
    fetchNextPage: vi.fn(),
    hasNextPage: false,
    isFetchingNextPage: false,
    refetch: vi.fn(),
    noResults: false,
    isDebouncing: false,
    hasResults: false,
  };
}

function defaultKeyboardNavReturn() {
  return {
    focusIndex: null,
    containerRef: { current: null },
    handleKeyDown: vi.fn(),
    setFocusIndex: vi.fn(),
  };
}

/* ------------------------------------------------------------------ */
/*  Fixtures                                                          */
/* ------------------------------------------------------------------ */

function createMockItems(count: number): MediaItem[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `item-${i}`,
    filename: `image-${i}.png`,
    path: `/images/image-${i}.png`,
    mime_type: 'image/png',
    thumbnail_url: `/api/v1/media/item-${i}/thumbnail?width=200`,
    width: 1024,
    height: 768,
    file_size: 1024 * 50,
    created_at: '2026-01-15T10:00:00Z',
    modified_at: '2026-01-15T10:00:00Z',
  }));
}

/* ------------------------------------------------------------------ */
/*  Responsive grid class list (canonical source of truth)            */
/* ------------------------------------------------------------------ */

const EXPECTED_GRID_CLASSES = [
  'grid',
  'grid-cols-2',
  'sm:grid-cols-3',
  'lg:grid-cols-4',
  'xl:grid-cols-5',
  'gap-3',
];

/* ------------------------------------------------------------------ */
/*  Tests                                                             */
/* ------------------------------------------------------------------ */

describe('ThumbnailGrid – responsive grid layout', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseSearch.mockReturnValue(defaultUseSearchReturn());
    mockUseKeyboardNav.mockReturnValue(defaultKeyboardNavReturn());
  });

  /* ---------- Responsive class verification ---------- */

  it('renders the ListContainer div with all expected responsive classes', () => {
    mockUseInfiniteMedia.mockReturnValue({
      allItems: createMockItems(10),
      isLoading: false,
      isError: false,
      error: null,
      fetchNextPage: vi.fn(),
      hasNextPage: false,
      isFetchingNextPage: false,
      refetch: vi.fn(),
    });

    const { container } = render(<ThumbnailGrid onItemClick={vi.fn()} />);

    // The ListContainer div is the direct child of the VirtuosoGrid mock's
    // wrapper, so we search for an element whose className contains all
    // expected responsive tokens.
    const gridEl = container.querySelector('[class*="grid"]');
    expect(gridEl).not.toBeNull();

    const classList = gridEl!.getAttribute('class')?.split(/\s+/) ?? [];
    for (const cls of EXPECTED_GRID_CLASSES) {
      expect(classList).toContain(cls);
    }
  });

  it('includes responsive gap and padding classes', () => {
    mockUseInfiniteMedia.mockReturnValue({
      allItems: createMockItems(10),
      isLoading: false,
      isError: false,
      error: null,
      fetchNextPage: vi.fn(),
      hasNextPage: false,
      isFetchingNextPage: false,
      refetch: vi.fn(),
    });

    const { container } = render(<ThumbnailGrid onItemClick={vi.fn()} />);
    const gridEl = container.querySelector('[class*="grid"]');
    expect(gridEl).toHaveClass('gap-3');
    expect(gridEl).toHaveClass('p-3');
  });

  it('has 2 columns on mobile (default) and scales up at each breakpoint', () => {
    mockUseInfiniteMedia.mockReturnValue({
      allItems: createMockItems(10),
      isLoading: false,
      isError: false,
      error: null,
      fetchNextPage: vi.fn(),
      hasNextPage: false,
      isFetchingNextPage: false,
      refetch: vi.fn(),
    });

    const { container } = render(<ThumbnailGrid onItemClick={vi.fn()} />);
    const gridEl = container.querySelector('[class*="grid"]');

    // Default (mobile first)
    expect(gridEl).toHaveClass('grid-cols-2');
    // sm (640px+)
    expect(gridEl).toHaveClass('sm:grid-cols-3');
    // lg (1024px+)
    expect(gridEl).toHaveClass('lg:grid-cols-4');
    // xl (1280px+)
    expect(gridEl).toHaveClass('xl:grid-cols-5');
  });

  /* ---------- Render states ---------- */

  it('renders skeleton grid with matching responsive classes while loading', () => {
    mockUseInfiniteMedia.mockReturnValue({
      allItems: [],
      isLoading: true,
      isError: false,
      error: null,
      fetchNextPage: vi.fn(),
      hasNextPage: false,
      isFetchingNextPage: false,
      refetch: vi.fn(),
    });

    const { container } = render(<ThumbnailGrid onItemClick={vi.fn()} />);

    // Skeleton grid should use the exact same responsive classes.
    // The SkeletonGrid component wraps cards in a grid container nested
    // inside a padding wrapper.
    const skeletonGrid = container
      .querySelector('[class*="grid"] [class*="animate-pulse"]')
      ?.closest('[class*="grid"]');
    expect(skeletonGrid).not.toBeNull();

    const classList = skeletonGrid!.getAttribute('class')?.split(/\s+/) ?? [];
    for (const cls of EXPECTED_GRID_CLASSES) {
      expect(classList).toContain(cls);
    }
  });

  it('renders empty state when no items are available', () => {
    mockUseInfiniteMedia.mockReturnValue({
      allItems: [],
      isLoading: false,
      isError: false,
      error: null,
      fetchNextPage: vi.fn(),
      hasNextPage: false,
      isFetchingNextPage: false,
      refetch: vi.fn(),
    });

    render(<ThumbnailGrid onItemClick={vi.fn()} />);
    expect(screen.getByText(/no media found/i)).toBeInTheDocument();
  });

  it('renders error state when fetch fails', () => {
    mockUseInfiniteMedia.mockReturnValue({
      allItems: [],
      isLoading: false,
      isError: true,
      error: new Error('Network failure'),
      fetchNextPage: vi.fn(),
      hasNextPage: false,
      isFetchingNextPage: false,
      refetch: vi.fn(),
    });

    render(<ThumbnailGrid onItemClick={vi.fn()} />);
    expect(screen.getByText(/network failure/i)).toBeInTheDocument();
  });
});
