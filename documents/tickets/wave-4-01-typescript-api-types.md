# Wave 4.1 — Define TypeScript API Types from Contract

| Field | Value |
|-------|-------|
| **Wave** | 4 — Frontend: Core Layout & Infinite Scroll |
| **Seq** | 01 |
| **Estimate** | 1 hour |
| **Depends on** | None (independent — based on API contract) |
| **Parallel** | No (foundation for all frontend tasks) |

---

## Overview

Define TypeScript type definitions that exactly match the API contract from §3 of the development plan. These types are the single source of truth for all API interactions and component props throughout the frontend.

## Prerequisites

- Frontend scaffolded (0.3)
- TypeScript strict mode enabled (0.3)

## Reference Files

- `documents/plans/development-plan.md` — §3.3 Data Models (MediaItem, Cursor Pagination Response, Search Response, SSE Event Format), §3.4 Query Parameters
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/types/
├── media.ts                     # MediaItem (list + detail), MediaMetadata
└── api.ts                       # PaginatedResponse, SearchResponse, SSE events, query params
```

## Acceptance Criteria (Pass/Fail)

- [ ] `media.ts` exports `MediaItem` type matching §3.3 list view (id, filename, path, mime_type, thumbnail_url, width, height, file_size, created_at, modified_at)
- [ ] `media.ts` exports `MediaItemDetail` type extending `MediaItem` with `file_url` and `metadata` fields
- [ ] `media.ts` exports `MediaMetadata` type (prompt, workflow — as `Record<string, unknown>`)
- [ ] `api.ts` exports `PaginatedResponse<T>` generic type:
  ```typescript
  { data: T[]; meta: { next_cursor: string | null; next_cursor_id: string | null; has_more: boolean; total: number; query?: string } }
  ```
- [ ] `api.ts` exports `SearchResponse` (= `PaginatedResponse<MediaItem> & { meta: { query: string } }`)
- [ ] `api.ts` exports `SseEvent` type matching §3.3 SSE Event Format (event_type + discriminator)
- [ ] `api.ts` exports `MediaListParams` and `SearchParams` types matching §3.4 Query Parameters
- [ ] All types pass `npx tsc --noEmit`
- [ ] Types use `readonly` where appropriate (immutable data from API)

## Implementation Notes

**`media.ts`:**
```typescript
export interface MediaItem {
  readonly id: string;
  readonly filename: string;
  readonly path: string;
  readonly mime_type: string;
  readonly thumbnail_url: string;
  readonly width: number | null;
  readonly height: number | null;
  readonly file_size: number;
  readonly created_at: string;    // ISO 8601
  readonly modified_at: string;   // ISO 8601
}

export interface MediaItemDetail extends MediaItem {
  readonly file_url: string;
  readonly metadata: MediaMetadata | null;
}

export interface MediaMetadata {
  readonly prompt: Record<string, unknown> | null;
  readonly workflow: Record<string, unknown> | null;
}
```

**`api.ts`:**
```typescript
export interface PaginationMeta {
  next_cursor: string | null;
  next_cursor_id: string | null;
  has_more: boolean;
  total: number;
  query?: string;
}

export interface PaginatedResponse<T> {
  data: T[];
  meta: PaginationMeta;
}

export interface MediaListParams {
  cursor?: string;
  cursor_id?: string;
  limit?: number;
  mime_type?: string;
}

export interface SearchParams {
  q: string;
  cursor?: string;
  cursor_id?: string;
  limit?: number;
}

// Discriminated union for SSE events
export type SseEvent =
  | { event: 'connected'; data: { timestamp: string } }
  | { event: 'file_created'; data: MediaItem }
  | { event: 'file_deleted'; data: { id: string; path: string } }
  | { event: 'file_modified'; data: { id: string; filename: string; metadata_updated: boolean } }
  | { event: 'indexing_complete'; data: { total: number; duration_ms: number } }
  | { event: 'lagged'; data: { skipped: number } };
```

**Additional types for frontend-only use:**
```typescript
// In api.ts
export interface IndexStats {
  total_files: number;
  total_size_bytes: number;
  by_mime_type: Record<string, number>;
  last_indexed_at: string | null;
  indexing_status: IndexProgress;
  watched_folders_count: number;
}

export interface IndexProgress {
  status: 'Idle' | 'Scanning' | 'Indexing' | 'Complete' | 'Error';
  total_files: number;
  processed_files: number;
  current_file: string | null;
  error_count: number;
}

export interface AppConfig {
  watched_folders: WatchedFolder[];
}

export interface WatchedFolder {
  path: string;
  label?: string;
}
```

**Important design decisions:**
- Use `readonly` on all API response types — data from the API should never be mutated
- Use `interface` (not `type`) for object types — better IDE support and error messages
- Snake_case in types matches the API exactly (Rust convention). Don't transform to camelCase unless there's a strong reason.
- Discriminated union for `SseEvent` enables exhaustive switch statements with TypeScript narrowing

## Test Strategy

- These are type definitions — validated by `npx tsc --noEmit`
- No runtime tests needed
- Types will be implicitly tested by all subsequent task implementations
