/**
 * @vitest-environment jsdom
 *
 * Tests for ConfigPanel stats-refresh behavior (wave-8-19, review R4).
 *
 * The panel must NOT poll /stats while indexing is idle. While an index run
 * is in progress it polls at a slow interval (20s) so live numbers advance
 * between SSE events, and the SSE `indexing_complete` event (via the global
 * useSseGridUpdates bridge) triggers an immediate refresh that also stops the
 * timer. Repeated open/close cycles must leak neither intervals nor
 * EventSource connections.
 */

import { render, act, cleanup } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { ReactNode } from 'react';
import { ConfigPanel } from '../config-panel';
import { useSseGridUpdates } from '@/hooks/use-sse-grid-updates';
import { MockEventSource } from '../../../test-utils/mock-event-source';
import type { IndexStats } from '@/types/api';

const mocks = vi.hoisted(() => ({ get: vi.fn() }));

vi.mock('@/api/client', () => ({ get: mocks.get }));

/** Build an IndexStats fixture with sensible idle defaults. */
function makeStats(overrides?: {
  indexing?: Partial<IndexStats['indexing']>;
  total?: number;
}): IndexStats {
  return {
    total: overrides?.total ?? 3,
    total_file_size: 5300,
    by_mime_type: { 'image/png': 3 },
    last_indexed_at: '2026-01-01T00:00:00Z',
    indexing: {
      status: 'Idle',
      total: 0,
      processed: 0,
      errors: [],
      ...overrides?.indexing,
    },
  };
}

/**
 * Mount ConfigPanel together with the global SSE bridge (as <App /> does),
 * sharing one QueryClient. Returns the number of /stats and /config calls
 * made so far, read live from the mocked `get`.
 */
function renderPanel(): { statsCalls: () => number; configCalls: () => number } {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  function SseBridge() {
    useSseGridUpdates();
    return null;
  }

  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <SseBridge />
        {children}
      </QueryClientProvider>
    );
  }

  render(<ConfigPanel onClose={() => {}} />, { wrapper: Wrapper });

  const callsByPath = (path: string) => mocks.get.mock.calls.filter(([p]) => p === path).length;

  return {
    statsCalls: () => callsByPath('/stats'),
    configCalls: () => callsByPath('/config'),
  };
}

/** Flush pending queries/timers deterministically under fake timers. */
async function advance(ms: number): Promise<void> {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

describe('ConfigPanel stats refresh', () => {
  beforeEach(() => {
    MockEventSource.reset();
    vi.stubGlobal('EventSource', MockEventSource);
    vi.useFakeTimers();
    mocks.get.mockClear();

    mocks.get.mockImplementation((path: string) => {
      if (path === '/config') {
        return Promise.resolve({ watched_folders: [] });
      }
      return Promise.resolve(makeStats());
    });
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('makes zero stats requests over 60s while indexing is idle', async () => {
    // Arrange — default mock returns Idle status.
    const { statsCalls } = renderPanel();
    await advance(0);
    expect(statsCalls()).toBe(1); // initial fetch only

    // Act — 60 seconds of idle time.
    await advance(60_000);

    // Assert — no polling whatsoever.
    expect(statsCalls()).toBe(1);
  });

  it('polls every 20s (not 5s) while indexing is active', async () => {
    // Arrange — an index run is in progress.
    mocks.get.mockImplementation((path: string) => {
      if (path === '/config') return Promise.resolve({ watched_folders: [] });
      return Promise.resolve(
        makeStats({ indexing: { status: 'Indexing', total: 100, processed: 1 } }),
      );
    });

    const { statsCalls } = renderPanel();
    await advance(0);
    expect(statsCalls()).toBe(1);

    // Act + Assert — one fetch after a full 20s interval…
    await advance(20_000);
    expect(statsCalls()).toBe(2);

    // …nothing mid-cycle at +15s…
    await advance(15_000);
    expect(statsCalls()).toBe(2);

    // …and exactly one more at the next interval boundary (+5s more).
    await advance(5_000);
    expect(statsCalls()).toBe(3);
  });

  it('does not treat Complete as active — a finished run does not poll', async () => {
    // Arrange — a run finished; the tracker stays on Complete until the next run.
    mocks.get.mockImplementation((path: string) => {
      if (path === '/config') return Promise.resolve({ watched_folders: [] });
      return Promise.resolve(makeStats({ indexing: { status: 'Complete' } }));
    });

    const { statsCalls } = renderPanel();
    await advance(0);
    expect(statsCalls()).toBe(1);

    // Act
    await advance(60_000);

    // Assert
    expect(statsCalls()).toBe(1);
  });

  it('refreshes once on indexing_complete SSE and stops the timer', async () => {
    // Arrange — indexing active, slow poll running.
    let currentIndexing: IndexStats['indexing'] = {
      status: 'Indexing',
      total: 100,
      processed: 40,
      errors: [],
    };
    mocks.get.mockImplementation((path: string) => {
      if (path === '/config') return Promise.resolve({ watched_folders: [] });
      return Promise.resolve(makeStats({ indexing: currentIndexing }));
    });

    renderPanel();
    await advance(0);
    await advance(20_000);
    expect(mocks.get.mock.calls.filter(([p]) => p === '/stats')).toHaveLength(2);

    // Act — the backend finishes the run and broadcasts the event; responses
    // now report Idle.
    currentIndexing = { status: 'Idle', total: 0, processed: 0, errors: [] };
    const es = MockEventSource.instances[0];
    expect(es).toBeDefined();
    act(() => {
      es!.triggerEvent('indexing_complete', { total: 100, duration_ms: 42_000 });
    });
    await advance(0);

    // Assert — the SSE event caused an immediate refetch (3rd call)…
    expect(mocks.get.mock.calls.filter(([p]) => p === '/stats')).toHaveLength(3);

    // …and with the run finished, the interval is gone for good.
    await advance(60_000);
    expect(mocks.get.mock.calls.filter(([p]) => p === '/stats')).toHaveLength(3);
  });

  it('leaks no intervals or SSE connections across repeated open/close', async () => {
    // Arrange — idle indexing.
    const { statsCalls } = renderPanel();
    await advance(0);
    expect(statsCalls()).toBe(1);
    cleanup();

    const { statsCalls: statsCalls2 } = renderPanel();
    await advance(0);
    expect(statsCalls2()).toBe(2); // first mount's fetch + fresh initial fetch

    // Act — close the second panel and let time pass.
    cleanup();
    await advance(60_000);

    // Assert — no further fetches after close, and exactly one EventSource
    // per mount (the App-owned bridge) — the panel itself opens none, and
    // unmount actually closes the connection.
    expect(statsCalls2()).toBe(2);
    expect(MockEventSource.instances).toHaveLength(2);
    expect(MockEventSource.instances[0]!.closed).toBe(true);
    expect(MockEventSource.instances[1]!.closed).toBe(true);
  });
});
