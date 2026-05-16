import React from 'react';
import '@testing-library/jest-dom/vitest';

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
      Array.from({ length: count }, (_, i) => React.createElement(Item, { key: i }, itemContent?.(i))),
    );
  },
}));
