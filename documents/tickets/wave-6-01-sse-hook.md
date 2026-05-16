# Wave 6.1 — Implement SSE Connection Hook

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 01 |
| **Estimate** | 2 hours |
| **Depends on** | 4.2 (API client) |
| **Parallel** | No |

---

## Overview

Implement a `useSse` hook that connects to the `/events` SSE endpoint and parses the event stream into typed events. The hook handles connection lifecycle (connect, disconnect, reconnect with exponential backoff) and exposes events via a Jotai atom or callback.

## Prerequisites

- SSE endpoint running on backend (3.7)
- Vite proxy configured for `/events` (0.5)
- API types for SSE events (4.1)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Endpoints (GET /events), §3.3 SSE Event Format, §5 Wave 6 task 6.1
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/hooks/
├── use-sse.ts                   # SSE connection hook
└── __tests__/
    └── use-sse.test.ts          # Hook tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Connects to `/api/v1/events` using `EventSource` API
- [ ] Parses incoming events into typed `SseEvent` objects (from 4.1)
- [ ] Calls `onEvent` callback for each parsed event
- [ ] Handles connection errors gracefully (logs, doesn't crash)
- [ ] **Reconnect with exponential backoff**: 1s, 2s, 4s, 8s, max 30s
- [ ] Connection status exposed: `status: 'connecting' | 'connected' | 'disconnected' | 'error'`
- [ ] Manual `connect()` and `disconnect()` functions
- [ ] Cleanup on unmount (close EventSource)
- [ ] Unit test: mock EventSource, verify events are parsed

## Implementation Notes

```typescript
import { useState, useEffect, useRef, useCallback } from 'react';
import type { SseEvent } from '../types/api';

type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

interface UseSseOptions {
  onEvent: (event: SseEvent) => void;
  onError?: (error: Event) => void;
  autoConnect?: boolean;
}

export function useSse({ onEvent, onError, autoConnect = true }: UseSseOptions) {
  const [status, setStatus] = useState<ConnectionStatus>('disconnected');
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectAttemptRef = useRef(0);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout>>();
  const maxReconnectDelay = 30_000; // 30 seconds

  const getReconnectDelay = useCallback(() => {
    const delay = Math.min(1000 * Math.pow(2, reconnectAttemptRef.current), maxReconnectDelay);
    reconnectAttemptRef.current++;
    return delay;
  }, []);

  const connect = useCallback(() => {
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
    }

    setStatus('connecting');

    const es = new EventSource('/api/v1/events');
    eventSourceRef.current = es;

    es.onopen = () => {
      setStatus('connected');
      reconnectAttemptRef.current = 0; // Reset backoff on successful connection
    };

    es.onmessage = (event) => {
      // Generic message handler (fallback — events with `event:` field use addEventListener)
      try {
        const parsed: SseEvent = JSON.parse(event.data);
        onEvent(parsed);
      } catch {
        // Some messages might not be JSON (e.g., keep-alive comments)
      }
    };

    // Named event listeners
    es.addEventListener('file_created', (event: MessageEvent) => {
      try {
        const data = JSON.parse(event.data);
        onEvent({ event: 'file_created', data });
      } catch { /* ignore malformed events */ }
    });

    es.addEventListener('file_deleted', (event: MessageEvent) => {
      try {
        const data = JSON.parse(event.data);
        onEvent({ event: 'file_deleted', data });
      } catch { /* ignore */ }
    });

    es.addEventListener('file_modified', (event: MessageEvent) => {
      try {
        const data = JSON.parse(event.data);
        onEvent({ event: 'file_modified', data });
      } catch { /* ignore */ }
    });

    es.addEventListener('indexing_complete', (event: MessageEvent) => {
      try {
        const data = JSON.parse(event.data);
        onEvent({ event: 'indexing_complete', data });
      } catch { /* ignore */ }
    });

    es.addEventListener('lagged', (event: MessageEvent) => {
      try {
        const data = JSON.parse(event.data);
        onEvent({ event: 'lagged', data });
      } catch { /* ignore */ }
    });

    es.onerror = (error) => {
      setStatus('error');
      onError?.(error);
      
      // EventSource auto-reconnects, but we implement manual backoff
      es.close();
      
      const delay = getReconnectDelay();
      reconnectTimerRef.current = setTimeout(() => {
        connect();
      }, delay);
    };
  }, [onEvent, onError, getReconnectDelay]);

  const disconnect = useCallback(() => {
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }
    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current);
    }
    setStatus('disconnected');
  }, []);

  useEffect(() => {
    if (autoConnect) {
      connect();
    }
    return () => disconnect();
  }, [autoConnect, connect, disconnect]);

  return { status, connect, disconnect };
}
```

**Usage in a Jotai atom-based approach (alternative to callback):**
```typescript
// In sse-atoms.ts
import { atom } from 'jotai';

export const sseEventsAtom = atom<SseEvent[]>([]);
export const sseStatusAtom = atom<ConnectionStatus>('disconnected');

// In a top-level component that wires SSE → atoms:
function SseBridge() {
  const setEvents = useSetAtom(sseEventsAtom);
  const setStatus = useSetAtom(sseStatusAtom);

  useSse({
    onEvent: (event) => {
      setEvents((prev) => [event, ...prev].slice(0, 100)); // Keep last 100 events
    },
    onError: () => setStatus('error'),
  });

  return null; // Headless component
}
```

## Test Strategy

```typescript
import { renderHook, act } from '@testing-library/react';
import { useSse } from '../use-sse';

// Create a mock EventSource
class MockEventSource {
  onopen: (() => void) | null = null;
  onerror: ((error: Event) => void) | null = null;
  listeners: Record<string, ((event: MessageEvent) => void)[]> = {};

  constructor(url: string) { /* store url */ }
  close() { /* cleanup */ }
  addEventListener(type: string, listener: (event: MessageEvent) => void) {
    if (!this.listeners[type]) this.listeners[type] = [];
    this.listeners[type].push(listener);
  }
}

describe('useSse', () => {
  beforeEach(() => {
    (globalThis as any).EventSource = MockEventSource;
  });

  it('calls onEvent when file_created is received', () => {
    const onEvent = vi.fn();
    
    renderHook(() => useSse({ onEvent }));

    // Simulate receiving an event
    act(() => {
      const es = (EventSource as any).mock.instances[0];
      es.listeners['file_created']?.forEach((fn: Function) => 
        fn({ data: JSON.stringify({ id: '1', filename: 'test.png' }) })
      );
    });

    expect(onEvent).toHaveBeenCalledWith({
      event: 'file_created',
      data: { id: '1', filename: 'test.png' },
    });
  });
});
```
