/**
 * @vitest-environment jsdom
 *
 * Tests for useSse — verifies SSE connection lifecycle, event parsing,
 * error recovery with exponential backoff, and cleanup on unmount.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useSse } from '../use-sse';

interface MockEventListener {
  (event: MessageEvent): void;
}

class MockEventSource {
  static instances: MockEventSource[] = [];

  onopen: (() => void) | null = null;
  onerror: ((error: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  listeners: Map<string, MockEventListener[]> = new Map();
  url: string;

  constructor(url: string) {
    this.url = url;
    MockEventSource.instances.push(this);
  }

  addEventListener(type: string, listener: EventListener) {
    const existing = this.listeners.get(type) ?? [];
    existing.push(listener as MockEventListener);
    this.listeners.set(type, existing);
  }

  close() {
    // cleanup
  }

  triggerOpen() {
    this.onopen?.();
  }

  triggerEvent(type: string, data: unknown) {
    const listeners = this.listeners.get(type) ?? [];
    const event = new MessageEvent('message', { data: JSON.stringify(data) });
    for (const listener of listeners) {
      listener(event);
    }
  }

  triggerError() {
    this.onerror?.(new Event('error'));
  }

  triggerMessage(data: unknown) {
    const event = new MessageEvent('message', { data: JSON.stringify(data) });
    this.onmessage?.(event);
  }

  static reset() {
    MockEventSource.instances = [];
  }
}

describe('useSse', () => {
  beforeEach(() => {
    MockEventSource.reset();
    vi.stubGlobal('EventSource', MockEventSource);
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('connects to SSE endpoint on mount', () => {
    // Arrange
    const onEvent = vi.fn();

    // Act
    renderHook(() => useSse({ onEvent }));

    // Assert
    expect(MockEventSource.instances.length).toBe(1);
    expect(MockEventSource.instances[0]!.url).toBe('/api/v1/events');
  });

  it('calls onEvent when file_created is received', () => {
    // Arrange
    const onEvent = vi.fn();
    renderHook(() => useSse({ onEvent }));
    const es = MockEventSource.instances[0]!;

    // Act
    act(() => {
      es.triggerEvent('file_created', {
        id: 'test-id',
        filename: 'test.png',
        path: 'test.png',
        mime_type: 'image/png',
        thumbnail_url: '/thumb',
        width: 100,
        height: 200,
        file_size: 1000,
        created_at: '2025-01-01T00:00:00Z',
        modified_at: '2025-01-01T00:00:00Z',
      });
    });

    // Assert
    expect(onEvent).toHaveBeenCalledWith(
      expect.objectContaining({
        event: 'file_created',
        data: expect.objectContaining({ id: 'test-id', filename: 'test.png' }),
      }),
    );
  });

  it('reconnects with exponential backoff on error', () => {
    // Arrange
    const onEvent = vi.fn();
    renderHook(() => useSse({ onEvent }));
    expect(MockEventSource.instances.length).toBe(1);

    // Act — trigger error
    const es = MockEventSource.instances[0]!;
    act(() => {
      es.triggerError();
    });

    // First reconnect delay = 1s (2^0 * 1000)
    act(() => {
      vi.advanceTimersByTime(1000);
    });

    // Assert — a new EventSource instance should be created
    expect(MockEventSource.instances.length).toBeGreaterThanOrEqual(2);
  });

  it('updates connection status', () => {
    // Arrange
    const onEvent = vi.fn();
    const { result } = renderHook(() => useSse({ onEvent }));

    // Assert — initial status is 'connecting'
    expect(result.current.status).toBe('connecting');

    // Act — trigger open
    act(() => {
      MockEventSource.instances[0]?.triggerOpen();
    });

    // Assert — status becomes 'connected'
    expect(result.current.status).toBe('connected');
  });

  it('cleans up on unmount', () => {
    // Arrange
    const onEvent = vi.fn();
    const { unmount } = renderHook(() => useSse({ onEvent }));
    const es = MockEventSource.instances[0]!;
    const closeSpy = vi.spyOn(es, 'close');

    // Act
    unmount();

    // Assert
    expect(closeSpy).toHaveBeenCalled();
  });

  it('does not connect when autoConnect is false', () => {
    // Arrange
    const onEvent = vi.fn();

    // Act
    renderHook(() => useSse({ onEvent, autoConnect: false }));

    // Assert
    expect(MockEventSource.instances.length).toBe(0);
  });

  it('calls onError callback on connection error', () => {
    // Arrange
    const onEvent = vi.fn();
    const onError = vi.fn();
    renderHook(() => useSse({ onEvent, onError }));
    const es = MockEventSource.instances[0]!;

    // Act
    act(() => {
      es.triggerError();
    });

    // Assert
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledWith(expect.any(Event));
  });

  it('parses all SSE event types', () => {
    // Arrange
    const onEvent = vi.fn();
    renderHook(() => useSse({ onEvent }));
    const es = MockEventSource.instances[0]!;

    // Act — trigger each event type
    act(() => {
      es.triggerEvent('connected', { timestamp: '2025-01-01T00:00:00Z' });
      es.triggerEvent('file_deleted', { id: 'del-id', path: 'del.png' });
      es.triggerEvent('file_modified', {
        id: 'mod-id',
        filename: 'mod.png',
        metadata_updated: true,
      });
      es.triggerEvent('indexing_complete', { total: 42, duration_ms: 1500 });
      es.triggerEvent('lagged', { skipped: 5 });
    });

    // Assert
    expect(onEvent).toHaveBeenCalledTimes(5);
    expect(onEvent).toHaveBeenCalledWith(
      expect.objectContaining({ event: 'connected' }),
    );
    expect(onEvent).toHaveBeenCalledWith(
      expect.objectContaining({ event: 'file_deleted' }),
    );
    expect(onEvent).toHaveBeenCalledWith(
      expect.objectContaining({ event: 'file_modified' }),
    );
    expect(onEvent).toHaveBeenCalledWith(
      expect.objectContaining({ event: 'indexing_complete' }),
    );
    expect(onEvent).toHaveBeenCalledWith(
      expect.objectContaining({ event: 'lagged' }),
    );
  });

  it('handles malformed JSON gracefully', () => {
    // Arrange
    const onEvent = vi.fn();
    renderHook(() => useSse({ onEvent }));
    const es = MockEventSource.instances[0]!;

    // Act — simulate onmessage with invalid JSON
    act(() => {
      const event = new MessageEvent('message', { data: 'not valid json' });
      es.onmessage?.(event);
    });

    // Assert — no crash, no calls
    expect(onEvent).not.toHaveBeenCalled();
  });
});
