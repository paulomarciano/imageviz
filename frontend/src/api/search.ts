/**
 * Search-related API functions.
 *
 * Provides a typed helper for full-text search across indexed media items,
 * leveraging the Tantivy-backed search endpoint.
 *
 * @module
 */

import { get } from './client.ts';
import type { MediaItem, PaginatedResponse, SearchParams } from '../types/index.ts';

/** Execute a full-text search with cursor-based pagination. */
export function searchMedia(params: SearchParams): Promise<PaginatedResponse<MediaItem>> {
  return get('/search', {
    q: params.q,
    cursor: params.cursor,
    cursor_id: params.cursor_id,
    limit: params.limit,
  });
}
