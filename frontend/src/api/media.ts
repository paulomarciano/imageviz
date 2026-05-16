/**
 * Media-related API functions.
 *
 * Provides typed helpers for listing media items, fetching a single item's
 * details, and constructing thumbnail / file URLs.
 *
 * @module
 */

import { get } from './client.ts';
import type {
  MediaItem,
  MediaItemDetail,
  PaginatedResponse,
  MediaListParams,
} from '../types/index.ts';

/** Fetch a paginated list of media items matching the supplied filters. */
export function fetchMediaList(
  params?: MediaListParams,
): Promise<PaginatedResponse<MediaItem>> {
  return get('/media', params as Record<string, string | number | undefined>);
}

/** Fetch full details for a single media item by its ID. */
export function fetchMediaItem(id: string): Promise<MediaItemDetail> {
  return get(`/media/${id}`);
}

/**
 * Build the thumbnail URL for a media item.
 *
 * This is a direct URL string (not an async fetch) so it can be used directly
 * in `<img src="…">` or as a `background-image`.
 */
export function fetchThumbnailUrl(id: string, width = 200): string {
  return `/api/v1/media/${id}/thumbnail?width=${width}`;
}

/** Build the file-download URL for a media item. */
export function fetchFileUrl(id: string): string {
  return `/api/v1/media/${id}/file`;
}
