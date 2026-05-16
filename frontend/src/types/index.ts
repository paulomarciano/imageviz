/**
 * Barrel exports for all shared type definitions.
 *
 * Re-exports types from media.ts and api.ts so consumers can import from a
 * single path: `import type { MediaItem, PaginatedResponse } from '../types'`.
 */

export type { MediaItem, MediaItemDetail, MediaMetadata } from './media.ts';

export type {
  PaginationMeta,
  PaginatedResponse,
  MediaListParams,
  SearchParams,
  SseEvent,
  IndexStats,
  IndexProgress,
  AppConfig,
  WatchedFolder,
} from './api.ts';
