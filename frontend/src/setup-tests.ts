import React from 'react';
import { vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

/*
 * Mock EventSource globally for jsdom-based tests. The useSse hook (and
 * useSseGridUpdates which wraps it) creates an EventSource on mount, but
 * jsdom does not implement the EventSource API.
 */
vi.stubGlobal(
  'EventSource',
  class MockEventSource {
    readonly url: string;
    onopen: (() => void) | null = null;
    onerror: ((error: Event) => void) | null = null;
    onmessage: ((event: MessageEvent) => void) | null = null;

    constructor(url: string) {
      this.url = url;
    }

    addEventListener() {
      /* noop — test-level mocks override this */
    }

    close() {
      /* noop */
    }
  },
);

/*
 * Mock VirtuosoGrid globally so integration tests (which render <App />)
 * don't need to perform real virtual-scroll measurements in jsdom.
 * Unit tests (e.g. thumbnail-grid) may override this with their own mock.
 *
 * NB: Must use React.createElement because vi.mock factory runs before
 * the JSX transform hook is installed.
 */
vi.mock('react-virtuoso', () => ({
  VirtuosoGrid: (props: Record<string, unknown>) => {
    const components = props.components as
      | {
          List?: React.ComponentType<{ style?: React.CSSProperties; children?: React.ReactNode }>;
          Item?: React.ComponentType<{ style?: React.CSSProperties; children?: React.ReactNode }>;
        }
      | undefined;
    const itemContent = props.itemContent as ((index: number) => React.ReactNode) | undefined;
    const totalCount = props.totalCount as number | undefined;
    const List = components?.List ?? 'div';
    const Item = components?.Item ?? 'div';
    const count = totalCount ?? 0;
    return React.createElement(
      List,
      null,
      Array.from({ length: count }, (_, i) =>
        React.createElement(Item, { key: i }, itemContent?.(i)),
      ),
    );
  },
}));
