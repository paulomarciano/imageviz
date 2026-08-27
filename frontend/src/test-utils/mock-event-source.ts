/**
 * Shared EventSource mock for SSE hook tests.
 *
 * Records instances and exposes helpers to trigger open/error/named events,
 * mimicking the subset of the EventSource API used by `useSse`.
 */

type MockEventListener = (event: MessageEvent) => void;

export class MockEventSource {
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
    // Use the SSE event type as the MessageEvent type so listeners that
    // inspect event.type see a realistic value.
    const event = new MessageEvent(type, { data: JSON.stringify(data) });
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
