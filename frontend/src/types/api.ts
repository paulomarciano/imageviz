/**
 * API-level type definitions for the ImageViz frontend.
 *
 * Covers pagination, SSE events, stats, config, and request parameters.
 * All fields use snake_case to match the backend serialization.
 */

import type { MediaItem } from './media.ts';

/** Cursor-based pagination metadata returned by list/search endpoints. */
export interface PaginationMeta {
  readonly next_cursor: string | null;
  readonly next_cursor_id: string | null;
  readonly has_more: boolean;
  readonly total: number;
  readonly query?: string;
}

/** Generic wrapper for paginated API responses. */
export interface PaginatedResponse<T> {
  readonly data: T[];
  readonly meta: PaginationMeta;
}

/** Query parameters for the media list endpoint. */
export interface MediaListParams {
  readonly cursor?: string;
  readonly cursor_id?: string;
  readonly limit?: number;
  readonly mime_type?: string;
}

/** Query parameters for the search endpoint. */
export interface SearchParams {
  readonly q: string;
  readonly cursor?: string;
  readonly cursor_id?: string;
  readonly limit?: number;
}

/**
 * Discriminated union representing parsed SSE events from /api/v1/events.
 *
 * The `event` field corresponds to the SSE event type, and `data` contains
 * the parsed JSON payload.
 */
export type SseEvent =
  | { readonly event: 'connected'; readonly data: { readonly timestamp: string } }
  | { readonly event: 'file_created'; readonly data: MediaItem }
  | {
      readonly event: 'file_deleted';
      readonly data: { readonly id: string; readonly path: string };
    }
  | {
      readonly event: 'file_modified';
      readonly data: {
        readonly id: string;
        readonly filename: string;
        readonly metadata_updated: boolean;
      };
    }
  | {
      readonly event: 'indexing_complete';
      readonly data: { readonly total: number; readonly duration_ms: number };
    }
  | { readonly event: 'lagged'; readonly data: { readonly skipped: number } };

/** Overall indexing statistics from the /api/v1/stats endpoint. */
export interface IndexStats {
  readonly total_files: number;
  readonly total_size_bytes: number;
  readonly by_mime_type: Record<string, number>;
  readonly last_indexed_at: string | null;
  readonly indexing_status: IndexProgress;
  readonly watched_folders_count: number;
}

/** Progress information for the current or last indexing run. */
export interface IndexProgress {
  readonly status: 'Idle' | 'Scanning' | 'Indexing' | 'Complete' | 'Error';
  readonly total_files: number;
  readonly processed_files: number;
  readonly current_file: string | null;
  readonly error_count: number;
}

/** Application configuration from the /api/v1/config endpoint. */
export interface AppConfig {
  readonly watched_folders: WatchedFolder[];
}

/** A single watched folder entry in the application configuration. */
export interface WatchedFolder {
  readonly path: string;
  readonly label?: string;
}
