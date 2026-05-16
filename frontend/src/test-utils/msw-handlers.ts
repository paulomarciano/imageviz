/**
 * MSW v2 request handlers for API mocking in frontend tests.
 *
 * Provides mock responses for all major API endpoints used by Wave 4
 * components and hooks: media listing, search, and health check.
 *
 * @module
 */

import { http, HttpResponse } from 'msw';
import type { MediaItem } from '../types/media';

/** Create a mock MediaItem with sensible defaults and optional overrides. */
function createMockMediaItem(id: string, overrides?: Partial<MediaItem>): MediaItem {
  return {
    id,
    filename: `image_${id}.png`,
    path: `2025/image_${id}.png`,
    mime_type: 'image/png',
    thumbnail_url: `/api/v1/media/${id}/thumbnail`,
    width: 896,
    height: 1216,
    file_size: 245_760,
    created_at: '2025-01-01T00:00:00Z',
    modified_at: '2025-01-01T00:00:00Z',
    ...overrides,
  };
}

/** MSW request handlers for the ImageViz API. */
export const handlers = [
  http.get('/api/v1/media', ({ request }) => {
    const url = new URL(request.url);
    const limit = parseInt(url.searchParams.get('limit') ?? '100');
    const cursor = url.searchParams.get('cursor');

    const items = Array.from({ length: limit }, (_, i) => createMockMediaItem(`mock-id-${i}`));

    return HttpResponse.json({
      data: items,
      meta: {
        next_cursor: cursor ? null : '2025-01-01T00:00:00Z',
        next_cursor_id: cursor ? null : `mock-id-${limit - 1}`,
        has_more: !cursor,
        total: 250,
      },
    });
  }),

  http.get('/api/v1/search', ({ request }) => {
    const url = new URL(request.url);
    const q = url.searchParams.get('q') ?? '';

    return HttpResponse.json({
      data: [createMockMediaItem('search-1', { filename: `${q}_result.png` })],
      meta: {
        next_cursor: null,
        next_cursor_id: null,
        has_more: false,
        total: 1,
        query: q,
      },
    });
  }),

  http.get('/api/v1/health', () => {
    return HttpResponse.json({ status: 'ok', version: '0.1.0' });
  }),
];
