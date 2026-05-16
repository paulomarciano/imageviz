import { useState, useEffect, useRef, useCallback } from 'react';
import type { SseEvent } from '../types/api';

type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

interface UseSseOptions {
  readonly onEvent: (event: SseEvent) => void;
  readonly onError?: (error: Event) => void;
  readonly autoConnect?: boolean;
}

interface UseSseReturn {
  readonly status: ConnectionStatus;
  readonly connect: () => void;
  readonly disconnect: () => void;
}

const MAX_RECONNECT_DELAY = 30_000;
const RECONNECT_BASE_DELAY = 1_000;

/**
 * React hook for connecting to the /api/v1/events SSE endpoint.
 *
 * Parses incoming events into typed `SseEvent` objects and provides
 * connection lifecycle management with exponential backoff reconnect.
 *
 * @param options.onEvent   - Callback invoked for every parsed SSE event.
 * @param options.onError   - Optional callback invoked on connection errors.
 * @param options.autoConnect - Whether to connect on mount (default true).
 */
export function useSse({
  onEvent,
  onError,
  autoConnect = true,
}: UseSseOptions): UseSseReturn {
  const [status, setStatus] = useState<ConnectionStatus>('disconnected');
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectAttemptRef = useRef(0);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout>>();

  const getReconnectDelay = useCallback(() => {
    const delay = Math.min(
      RECONNECT_BASE_DELAY * 2 ** reconnectAttemptRef.current,
      MAX_RECONNECT_DELAY,
    );
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
      reconnectAttemptRef.current = 0;
    };

    es.onmessage = (event: MessageEvent) => {
      try {
        const data: Record<string, unknown> = JSON.parse(event.data);
        if (data.event) {
          onEvent(data as unknown as SseEvent);
        } else {
          // Broadcast messages from the server that carry no event type.
          onEvent({
            event: 'connected',
            data: { timestamp: new Date().toISOString() },
          } as SseEvent);
        }
      } catch {
        // Some messages may not be JSON (e.g., keep-alive comments).
      }
    };

    // Named event listeners for specific SSE event types.
    const eventTypes = [
      'connected',
      'file_created',
      'file_deleted',
      'file_modified',
      'indexing_complete',
      'lagged',
    ] as const;

    for (const eventType of eventTypes) {
      es.addEventListener(
        eventType,
        ((event: MessageEvent) => {
          try {
            const data: unknown = JSON.parse(event.data);
            onEvent({ event: eventType, data } as SseEvent);
          } catch {
            // ignore malformed events
          }
        }) as EventListener,
      );
    }

    es.onerror = (error: Event) => {
      setStatus('error');
      onError?.(error);

      // Close and reconnect with exponential backoff.
      es.close();

      const delay = getReconnectDelay();
      reconnectTimerRef.current = setTimeout(() => {
        void connect();
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
      void connect();
    }
    return () => disconnect();
  }, [autoConnect, connect, disconnect]);

  return { status, connect, disconnect };
}
