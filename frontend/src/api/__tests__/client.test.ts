/**
 * @vitest-environment jsdom
 *
 * Tests for the typed API client — `get<T>()` and `put<T>()`. Verifies URL
 * construction, JSON request bodies, the shared error envelope (ApiError
 * with server-provided message), and network-error mapping.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get, put, ApiError } from '../client';

/** Stub global fetch with a canned Response. */
function mockFetch(response: Response | { error: unknown }): ReturnType<typeof vi.fn> {
  const fn = vi.fn(async () => {
    if (response instanceof Response) return response;
    throw response.error;
  });
  vi.stubGlobal('fetch', fn);
  return fn;
}

function jsonResponse(status: number, body: unknown, statusText = ''): Response {
  return new Response(JSON.stringify(body), {
    status,
    statusText,
    headers: { 'Content-Type': 'application/json' },
  });
}

describe('api client', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  describe('get<T>', () => {
    beforeEach(() => {
      mockFetch(jsonResponse(200, { hello: 'world' }));
    });

    it('requests the path under /api/v1 and decodes JSON', async () => {
      await expect(get<{ hello: string }>('/thing')).resolves.toEqual({ hello: 'world' });
      expect(fetch).toHaveBeenCalledWith(`${window.location.origin}/api/v1/thing`, undefined);
    });

    it('appends defined query params and skips undefined ones', async () => {
      await get('/media', { limit: 50, q: 'cat', cursor: undefined });
      const url = new URL(vi.mocked(fetch).mock.calls[0]![0] as string);
      expect(url.searchParams.get('limit')).toBe('50');
      expect(url.searchParams.get('q')).toBe('cat');
      expect(url.searchParams.has('cursor')).toBe(false);
    });

    it('throws ApiError with the server error message on non-OK', async () => {
      mockFetch(jsonResponse(404, { error: 'not found' }, 'Not Found'));
      const err = await get('/missing').catch((e: unknown) => e);
      expect(err).toBeInstanceOf(ApiError);
      expect((err as ApiError).status).toBe(404);
      expect((err as ApiError).message).toBe('not found');
    });
  });

  describe('put<T>', () => {
    it('PUTs a JSON body with Content-Type and decodes the response', async () => {
      const fetchMock = mockFetch(jsonResponse(200, { saved: true }));

      await expect(put<{ saved: boolean }>('/config', { watched_folders: [] })).resolves.toEqual({
        saved: true,
      });

      const [url, init] = vi.mocked(fetchMock).mock.calls[0] as unknown as [string, RequestInit];
      expect(url).toBe('/api/v1/config');
      expect(init.method).toBe('PUT');
      expect(init.headers).toMatchObject({ 'Content-Type': 'application/json' });
      expect(init.body).toBe(JSON.stringify({ watched_folders: [] }));
    });

    it('throws ApiError with the server error message on non-OK', async () => {
      mockFetch(jsonResponse(400, { error: 'invalid folder path' }, 'Bad Request'));
      const err = await put('/config', {}).catch((e: unknown) => e);
      expect(err).toBeInstanceOf(ApiError);
      expect((err as ApiError).status).toBe(400);
      expect((err as ApiError).message).toBe('invalid folder path');
    });

    it('falls back to statusText when the error body is not JSON', async () => {
      mockFetch(new Response('plain text', { status: 500, statusText: 'Internal Server Error' }));
      const err = await put('/config', {}).catch((e: unknown) => e);
      expect((err as ApiError).status).toBe(500);
      expect((err as ApiError).message).toBe('Internal Server Error');
    });

    it('maps network failures to ApiError with status 0', async () => {
      mockFetch({ error: new TypeError('Failed to fetch') });
      const err = await put('/config', {}).catch((e: unknown) => e);
      expect(err).toBeInstanceOf(ApiError);
      expect((err as ApiError).status).toBe(0);
      expect((err as ApiError).message).toContain('Network error');
    });
  });
});
