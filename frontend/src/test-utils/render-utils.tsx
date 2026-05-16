/**
 * Custom render utilities for ImageViz frontend tests.
 *
 * Provides `renderWithProviders` wrapping the UI in QueryClientProvider +
 * Jotai Provider, and `createMockMediaItem` for building test fixtures.
 *
 * Usage:
 * ```ts
 * import { renderWithProviders, createMockMediaItem } from '../../test-utils/render-utils';
 * ```
 *
 * @module
 */

import { type ReactElement } from 'react';
import { render, type RenderResult } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Provider as JotaiProvider, createStore } from 'jotai';
import type { MediaItem } from '../types/media';

interface RenderOptions {
  queryClient?: QueryClient;
}

/**
 * Render a React element wrapped in all required providers.
 *
 * By default creates a new QueryClient with `retry: false` (so tests don't
 * hang on failed queries) and a fresh Jotai store.
 */
export function renderWithProviders(ui: ReactElement, options?: RenderOptions): RenderResult {
  const queryClient =
    options?.queryClient ??
    new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
  const jotaiStore = createStore();

  function Wrapper({ children }: { children: React.ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <JotaiProvider store={jotaiStore}>{children}</JotaiProvider>
      </QueryClientProvider>
    );
  }

  return render(ui, { wrapper: Wrapper });
}

/** Build a mock MediaItem with sensible defaults and optional overrides. */
export function createMockMediaItem(overrides?: Partial<MediaItem>): MediaItem {
  return {
    id: 'test-id',
    filename: 'test.png',
    path: '2025/test.png',
    mime_type: 'image/png',
    thumbnail_url: '/api/v1/media/test-id/thumbnail',
    width: 896,
    height: 1216,
    file_size: 245_760,
    created_at: '2025-01-01T00:00:00Z',
    modified_at: '2025-01-01T00:00:00Z',
    ...overrides,
  };
}
