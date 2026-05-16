# Wave 4.2 — Implement API Client Layer

| Field | Value |
|-------|-------|
| **Wave** | 4 — Frontend: Core Layout & Infinite Scroll |
| **Seq** | 02 |
| **Estimate** | 1.5 hours |
| **Depends on** | 4.1 (API types) |
| **Parallel** | No |

---

## Overview

Implement a typed API client layer that wraps `fetch` with base URL handling, error handling, and typed response parsing. This provides clean, type-safe functions for each backend endpoint. All subsequent frontend hooks and components will use these client functions.

## Prerequisites

- TypeScript API types (4.1)
- Vite proxy configured (0.5) — requests go to `/api/v1/...`

## Reference Files

- `documents/plans/development-plan.md` — §3 API Contract (all endpoints), §12 project structure (api/client.ts, api/media.ts, api/search.ts)
- `.opencode/context/core/standards/code-quality.md` — functional patterns, explicit error handling

## Deliverables

```
frontend/src/api/
├── client.ts                    # Fetch wrapper (base URL, error handling, JSON parsing)
├── media.ts                     # Media API functions
└── search.ts                    # Search API functions
```

## Acceptance Criteria (Pass/Fail)

- [ ] `client.ts` exports a `get<T>(path, params?)` function that returns `Promise<T>`
- [ ] Base URL is `/api/v1` (no hardcoded localhost — works through Vite proxy)
- [ ] Query parameters are properly serialized (URLSearchParams, skip null/undefined)
- [ ] HTTP errors (4xx, 5xx) are thrown as typed `ApiError` with status code and message
- [ ] Network errors are caught and re-thrown as `ApiError`
- [ ] `media.ts` exports:
  - `fetchMediaList(params?)` → `PaginatedResponse<MediaItem>`
  - `fetchMediaItem(id)` → `MediaItemDetail`
  - `fetchThumbnailUrl(id, width?)` → `string` (URL, not data)
  - `fetchFileUrl(id)` → `string` (URL, not data)
- [ ] `search.ts` exports:
  - `searchMedia(params)` → `PaginatedResponse<MediaItem>`
- [ ] All functions are properly typed (no `any`)
- [ ] Functions are pure (no side effects, no global state)

## Implementation Notes

**`client.ts` — typed fetch wrapper:**
```typescript
const BASE_URL = '/api/v1';

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

export async function get<T>(
  path: string,
  params?: Record<string, string | number | undefined>,
): Promise<T> {
  const url = new URL(`${BASE_URL}${path}`, window.location.origin);
  
  if (params) {
    Object.entries(params).forEach(([key, value]) => {
      if (value !== undefined && value !== null) {
        url.searchParams.set(key, String(value));
      }
    });
  }
  
  let response: Response;
  try {
    response = await fetch(url.toString());
  } catch (error) {
    throw new ApiError(0, `Network error: ${error instanceof Error ? error.message : 'Unknown'}`);
  }
  
  if (!response.ok) {
    let message = response.statusText;
    try {
      const body = await response.json();
      message = body.error || body.message || message;
    } catch { /* body not JSON */ }
    throw new ApiError(response.status, message);
  }
  
  return response.json();
}
```

**`media.ts`:**
```typescript
import { get } from './client';
import type { MediaItem, MediaItemDetail, PaginatedResponse, MediaListParams } from '../types';

export function fetchMediaList(
  params?: MediaListParams,
): Promise<PaginatedResponse<MediaItem>> {
  return get('/media', params as Record<string, string | number | undefined>);
}

export function fetchMediaItem(id: string): Promise<MediaItemDetail> {
  return get(`/media/${id}`);
}

export function fetchThumbnailUrl(id: string, width = 200): string {
  return `/api/v1/media/${id}/thumbnail?width=${width}`;
}

export function fetchFileUrl(id: string): string {
  return `/api/v1/media/${id}/file`;
}
```

**`search.ts`:**
```typescript
import { get } from './client';
import type { MediaItem, PaginatedResponse, SearchParams } from '../types';
import type { SearchResponse } from '../types/api';

export function searchMedia(
  params: SearchParams,
): Promise<PaginatedResponse<MediaItem>> {
  return get('/search', {
    q: params.q,
    cursor: params.cursor,
    cursor_id: params.cursor_id,
    limit: params.limit,
  });
}
```

**Design notes:**
- `fetchThumbnailUrl` and `fetchFileUrl` return URL strings, not fetched data — these URLs are used in `<img src>` and drag operations
- The `get<T>` function is generic — new endpoints can be added by just calling `get<ResponseType>(path, params)`
- `ApiError` class preserves status code for UI error handling

## Test Strategy

```typescript
// frontend/src/api/__tests__/client.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get, ApiError } from '../client';

describe('get', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('fetches and parses JSON response', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ data: 'test' }),
    } as Response);

    const result = await get('/test');
    expect(result).toEqual({ data: 'test' });
  });

  it('throws ApiError on HTTP error', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: false,
      status: 404,
      statusText: 'Not Found',
      json: () => Promise.resolve({}),
    } as Response);

    await expect(get('/test')).rejects.toThrow(ApiError);
    await expect(get('/test')).rejects.toMatchObject({ status: 404 });
  });

  it('serializes query parameters', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({}),
    } as Response);

    await get('/test', { foo: 'bar', baz: 42 });

    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toContain('foo=bar');
    expect(url).toContain('baz=42');
  });
});
```
