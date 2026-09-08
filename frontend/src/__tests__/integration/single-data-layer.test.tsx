/**
 * @vitest-environment jsdom
 *
 * Wave 8.10 — Single Data Layer (Jotai atom) integration tests.
 *
 * Regression coverage for code review §2 D7: App.tsx used to run duplicate
 * useInfiniteMedia/useSearch hooks with drifted parameters (hardcoded
 * 'recency' + no mime filter), causing two search requests per keystroke and
 * detail-view navigation order that diverged from the grid.
 *
 * The grid now writes the flattened item list into `mediaDataAtom`; App's
 * DetailView reads it. These tests pin the contract:
 *   1. exactly one /api/v1/search request per query change, regardless of
 *      sort / mime-filter state
 *   2. detail-view arrow-key navigation order == grid display order
 *   3. an open detail view never navigates into a previous query's items
 *   4. infinite-scroll page accumulation lands in the atom
 */

import React, { useEffect, useRef } from 'react';
import { screen, waitFor, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import { useAtomValue } from 'jotai';
import { handlers } from '../../test-utils/msw-handlers';
import { renderWithProviders } from '../../test-utils/render-utils';
import App from '../../App';
import { mediaDataAtom } from '../../store/media-data-atoms';
import type { MediaItem, MediaItemDetail } from '../../types/media';

/* ------------------------------------------------------------------ */
/*  Local VirtuosoGrid mock: renders all items, fires endReached once  */
/* ------------------------------------------------------------------ */

vi.mock('react-virtuoso', () => ({
  VirtuosoGrid: (props: Record<string, unknown>) => {
    const components = props.components as
      | {
          List?: React.ComponentType<{ children?: React.ReactNode }>;
          Item?: React.ComponentType<{ children?: React.ReactNode }>;
        }
      | undefined;
    const itemContent = props.itemContent as ((index: number) => React.ReactNode) | undefined;
    const endReached = props.endReached as (() => void) | undefined;
    const totalCount = (props.totalCount as number) ?? 0;

    const endReachedRef = useRef(endReached);
    endReachedRef.current = endReached;
    const firedRef = useRef(false);

    useEffect(() => {
      if (firedRef.current) return;
      firedRef.current = true;
      const timer = setTimeout(() => endReachedRef.current?.(), 50);
      return () => clearTimeout(timer);
    }, []);

    const List = components?.List ?? 'div';
    const Item = components?.Item ?? 'div';
    return React.createElement(
      List,
      null,
      Array.from({ length: totalCount }, (_, i) =>
        React.createElement(Item, { key: i }, itemContent?.(i)),
      ),
    );
  },
}));

/* ------------------------------------------------------------------ */
/*  MSW server                                                        */
/* ------------------------------------------------------------------ */

const server = setupServer(...handlers);

beforeAll(() => {
  // jsdom does not implement HTMLMediaElement.prototype.play — its stub
  // returns undefined, but VideoViewer calls play().catch(...) on mount
  // (same stub pattern as video-viewer.test.tsx).
  HTMLVideoElement.prototype.play = vi.fn(() => Promise.resolve());
  server.listen({ onUnhandledRequest: 'bypass' });
});
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

/* ------------------------------------------------------------------ */
/*  Fixtures                                                          */
/* ------------------------------------------------------------------ */

function item(id: string, filename: string, mime_type: string): MediaItem {
  return {
    id,
    filename,
    path: `2025/${filename}`,
    mime_type,
    thumbnail_url: `/api/v1/media/${id}/thumbnail`,
    width: 640,
    height: 480,
    file_size: 1024,
    created_at: '2026-01-01T00:00:00Z',
    modified_at: '2026-01-01T00:00:00Z',
  };
}

function detailResponse(media: MediaItem): MediaItemDetail {
  return {
    ...media,
    file_url: `/api/v1/media/${media.id}/file`,
    metadata: null,
  };
}

function paginated(data: MediaItem[], query?: string) {
  return HttpResponse.json({
    data,
    meta: {
      next_cursor: null,
      next_cursor_id: null,
      has_more: false,
      total: data.length,
      ...(query !== undefined && { query }),
    },
  });
}

/**
 * Mixed-type fixtures. Mimics a real backend where parameters matter:
 *  - sort=score  → natural (score) order [a, b, c, d]
 *  - sort=recency → reversed order [d, c, b, a]
 *  - mime_type=video/% → videos only
 */
const SCORE_ORDER: MediaItem[] = [
  item('vid-a', 'a-first.mp4', 'video/mp4'),
  item('vid-b', 'b-second.mp4', 'video/mp4'),
  item('vid-c', 'c-third.mp4', 'video/mp4'),
  item('img-d', 'd-fourth.png', 'image/png'),
];

/** Search handler whose response depends on q, sort, and mime_type. */
function paramSensitiveSearch({ request }: { request: Request }) {
  const url = new URL(request.url);
  const q = url.searchParams.get('q') ?? '';
  const mime = url.searchParams.get('mime_type');
  const sort = url.searchParams.get('sort');

  let data = SCORE_ORDER.filter((i) => i.filename.includes(q));
  if (mime === 'video/%') data = data.filter((i) => i.mime_type.startsWith('video/'));
  if (sort !== 'score') data = [...data].reverse();

  return paginated(data, q);
}

/** Records /api/v1/media/:id (detail) fetches and answers with a valid body. */
function trackDetailFetches(into: string[]) {
  return http.get('/api/v1/media/:id', ({ params }) => {
    into.push(params.id as string);
    const media = item(params.id as string, `${params.id}.png`, 'image/png');
    return HttpResponse.json(detailResponse(media));
  });
}

/* ------------------------------------------------------------------ */
/*  Atom probe — shares the Jotai store with <App />                  */
/* ------------------------------------------------------------------ */

function MediaDataProbe() {
  const { mode, items } = useAtomValue(mediaDataAtom);
  return (
    <div
      data-testid="media-data-probe"
      data-mode={mode}
      data-ids={items.map((i) => i.id).join(',')}
    />
  );
}

/** Open the search input and wait for one debounced query change to land. */
async function searchFor(input: HTMLElement, query: string, expectText: string) {
  await userEvent.type(input, query);
  await waitFor(() => expect(screen.getByText(expectText)).toBeInTheDocument(), {
    timeout: 2000,
  });
}

/* ------------------------------------------------------------------ */
/*  Tests                                                             */
/* ------------------------------------------------------------------ */

describe('Single data layer (Wave 8.10)', () => {
  it('fires exactly one search request per query change, even with non-default sort + mime filter', async () => {
    const searchUrls: string[] = [];
    server.use(
      http.get('/api/v1/search', ({ request }) => {
        searchUrls.push(request.url);
        return paramSensitiveSearch({ request });
      }),
    );

    renderWithProviders(
      <>
        <App />
        <MediaDataProbe />
      </>,
    );

    // Non-default sort + mime filter — the state the old App.tsx hooks ignored.
    await userEvent.click(screen.getByRole('radio', { name: 'Relevance' }));
    await userEvent.click(screen.getByRole('radio', { name: 'Videos' }));

    const input = screen.getByPlaceholderText('Search media...');
    await searchFor(input, 'mp4', 'a-first.mp4');

    // Second query change: wait for its request to land (text alone is
    // ambiguous — "mp4" results already contain "b-second.mp4"), let any
    // same-tick duplicates flush, then assert the exact request log.
    await userEvent.type(input, ' b'); // query becomes "mp4 b"
    await waitFor(
      () =>
        expect(searchUrls.filter((u) => new URL(u).searchParams.get('q') === 'mp4 b')).toHaveLength(
          1,
        ),
      { timeout: 2000 },
    );
    await new Promise((r) => setTimeout(r, 300));

    // Exactly one request per debounced query change.
    expect(searchUrls).toHaveLength(2);
    expect(searchUrls.map((u) => new URL(u).searchParams.get('q'))).toEqual(['mp4', 'mp4 b']);
    for (const url of searchUrls) {
      const params = new URL(url).searchParams;
      expect(params.get('sort')).toBe('score');
      expect(params.get('mime_type')).toBe('video/%');
    }
  });

  it('navigates the detail view in grid display order with sort=score + video filter', async () => {
    const detailFetches: string[] = [];
    server.use(http.get('/api/v1/search', paramSensitiveSearch), trackDetailFetches(detailFetches));

    renderWithProviders(<App />);

    await userEvent.click(screen.getByRole('radio', { name: 'Relevance' }));
    await userEvent.click(screen.getByRole('radio', { name: 'Videos' }));

    const input = screen.getByPlaceholderText('Search media...');
    await searchFor(input, 'mp4', 'a-first.mp4');

    // Grid displays [a-first, b-second, c-third]; open the first card.
    await userEvent.click(screen.getByRole('button', { name: 'View a-first.mp4' }));
    await waitFor(() => expect(detailFetches).toEqual(['vid-a']));

    fireEvent.keyDown(window, { key: 'ArrowRight' });
    await waitFor(() => expect(detailFetches).toEqual(['vid-a', 'vid-b']));

    fireEvent.keyDown(window, { key: 'ArrowRight' });
    await waitFor(() => expect(detailFetches).toEqual(['vid-a', 'vid-b', 'vid-c']));

    // Grid order exactly — never the reversed recency order the old
    // duplicate hook produced.
    expect(detailFetches).toEqual(['vid-a', 'vid-b', 'vid-c']);
  });

  it('never serves previous-query items to an open detail view after the query changes', async () => {
    const detailFetches: string[] = [];
    server.use(
      http.get('/api/v1/search', ({ request }) => {
        const q = new URL(request.url).searchParams.get('q') ?? '';
        const data =
          q === 'alpha'
            ? [
                item('alpha-1', 'alpha-one.png', 'image/png'),
                item('alpha-2', 'alpha-two.png', 'image/png'),
              ]
            : q === 'beta'
              ? [item('beta-1', 'beta-one.png', 'image/png')]
              : [];
        return paginated(data, q);
      }),
      trackDetailFetches(detailFetches),
    );

    renderWithProviders(<App />);

    const input = screen.getByPlaceholderText('Search media...');
    await searchFor(input, 'alpha', 'alpha-one.png');

    await userEvent.click(screen.getByRole('button', { name: 'View alpha-one.png' }));
    await waitFor(() => expect(detailFetches).toEqual(['alpha-1']));

    // Change the query while the detail view is open.
    await userEvent.clear(input);
    await searchFor(input, 'beta', 'beta-one.png');

    fireEvent.keyDown(window, { key: 'ArrowRight' });

    // Only the selected item was ever fetched — never alpha-2 (previous
    // query's list) and nothing from the beta list either.
    expect(detailFetches.every((id) => id === 'alpha-1')).toBe(true);
  });

  it('accumulates infinite-scroll pages into the atom in grid order', async () => {
    server.use(
      http.get('/api/v1/media', ({ request }) => {
        const cursor = new URL(request.url).searchParams.get('cursor');
        const data = cursor
          ? [item('p3', 'page3.png', 'image/png'), item('p4', 'page4.png', 'image/png')]
          : [item('p1', 'page1.png', 'image/png'), item('p2', 'page2.png', 'image/png')];
        const hasMore = !cursor;
        return HttpResponse.json({
          data,
          meta: {
            next_cursor: hasMore ? 'cursor-2' : null,
            next_cursor_id: hasMore ? 'p2' : null,
            has_more: hasMore,
            total: 4,
          },
        });
      }),
    );

    renderWithProviders(
      <>
        <App />
        <MediaDataProbe />
      </>,
    );

    const probe = screen.getByTestId('media-data-probe');

    // Page 1 lands in the atom.
    await waitFor(() => expect(probe).toHaveAttribute('data-ids', 'p1,p2'), { timeout: 2000 });
    expect(probe).toHaveAttribute('data-mode', 'browse');

    // The local VirtuosoGrid mock fires endReached after mount → page 2.
    await waitFor(() => expect(probe).toHaveAttribute('data-ids', 'p1,p2,p3,p4'), {
      timeout: 2000,
    });
  });
});
