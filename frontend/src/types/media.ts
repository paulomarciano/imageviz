/**
 * Media-related type definitions for the ImageViz frontend.
 *
 * These types map to the Rust backend's REST API response shapes at /api/v1.
 * All fields use snake_case to match the backend serialization.
 */

/** Summary representation of a media item (used in list/search responses). */
export interface MediaItem {
  readonly id: string;
  readonly filename: string;
  readonly path: string;
  readonly mime_type: string;
  readonly thumbnail_url: string;
  readonly width: number | null;
  readonly height: number | null;
  readonly file_size: number;
  readonly created_at: string; // ISO 8601
  readonly modified_at: string; // ISO 8601
}

/** Full detail of a media item (used in single-item responses). */
export interface MediaItemDetail extends MediaItem {
  readonly file_url: string;
  readonly metadata: MediaMetadata | null;
}

/** Arbitrary metadata extracted from media files (e.g., PNG tEXt chunks, video metadata). */
export interface MediaMetadata {
  readonly prompt: Record<string, unknown> | null;
  readonly workflow: Record<string, unknown> | null;
}
