/**
 * Typed fetch wrapper for the ImageViz REST API.
 *
 * All requests are directed at /api/v1 (proxied by Vite to the Rust backend
 * at localhost:3001). The module exposes a single `get<T>()` helper and a
 * custom `ApiError` class for consistent error handling across callers.
 *
 * @module
 */

const BASE_URL = '/api/v1';

/** Structured error returned when an API call fails. */
export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

/**
 * Perform a typed GET request against the API.
 *
 * Query parameters are appended automatically. `undefined` / `null` values are
 * skipped so callers can spread partial filter objects without explicit guards.
 *
 * @param path    – URL path relative to `/api/v1` (e.g. `/media`).
 * @param params  – Optional query-parameter map. Falsy values are omitted.
 * @returns       – The decoded JSON body typed as `T`.
 */
export async function get<T>(
  path: string,
  params?: Record<string, string | number | undefined>,
): Promise<T> {
  const url = new URL(`${BASE_URL}${path}`, window.location.origin);

  if (params) {
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined && value !== null) {
        url.searchParams.set(key, String(value));
      }
    }
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
      const body: unknown = await response.json();
      if (body && typeof body === 'object') {
        message =
          ((body as Record<string, unknown>).error as string) ??
          ((body as Record<string, unknown>).message as string) ??
          message;
      }
    } catch {
      // response body is not JSON; fall back to statusText
    }
    throw new ApiError(response.status, message);
  }

  return response.json() as Promise<T>;
}
