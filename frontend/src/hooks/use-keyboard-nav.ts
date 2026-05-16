import { useCallback, useState, useEffect, useRef } from 'react';

interface UseKeyboardNavOptions {
  readonly itemCount: number;
  readonly columns: number;
  readonly onSelect: (index: number) => void;
  readonly onOpen: (index: number) => void;
}

export interface UseKeyboardNavResult {
  readonly focusIndex: number | null;
  readonly containerRef: React.RefObject<HTMLDivElement | null>;
  readonly handleKeyDown: (e: React.KeyboardEvent) => void;
  readonly setFocusIndex: (index: number | null) => void;
}

/**
 * Manages keyboard navigation in a grid layout using the roving tabindex pattern.
 *
 * Arrow keys move focus between items, Enter opens an item, Space toggles
 * selection, and Home/End jump to the first/last item. Focus wraps at grid
 * boundaries. Uses `data-grid-index` attributes on grid items for focus
 * management, making it compatible with virtualized grids.
 *
 * @param itemCount - Total number of items in the grid.
 * @param columns   - Number of columns in the grid layout.
 * @param onSelect  - Callback invoked with the focused index on Space.
 * @param onOpen    - Callback invoked with the focused index on Enter.
 */
export function useKeyboardNav({
  itemCount,
  columns,
  onSelect,
  onOpen,
}: UseKeyboardNavOptions): UseKeyboardNavResult {
  const [focusIndex, setFocusIndex] = useState<number | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

  const moveFocus = useCallback(
    (current: number, direction: 'up' | 'down' | 'left' | 'right') => {
      let next: number;
      switch (direction) {
        case 'right':
          next = (current + 1) % itemCount;
          break;
        case 'left':
          next = (current - 1 + itemCount) % itemCount;
          break;
        case 'down':
          next = Math.min(current + columns, itemCount - 1);
          break;
        case 'up':
          next = Math.max(current - columns, 0);
          break;
      }
      setFocusIndex(next);

      const element = document.querySelector(`[data-grid-index="${next}"]`);
      element?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    },
    [itemCount, columns],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (focusIndex === null) {
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          setFocusIndex(0);
          e.preventDefault();
        }
        return;
      }

      switch (e.key) {
        case 'ArrowRight':
          moveFocus(focusIndex, 'right');
          e.preventDefault();
          break;
        case 'ArrowLeft':
          moveFocus(focusIndex, 'left');
          e.preventDefault();
          break;
        case 'ArrowDown':
          moveFocus(focusIndex, 'down');
          e.preventDefault();
          break;
        case 'ArrowUp':
          moveFocus(focusIndex, 'up');
          e.preventDefault();
          break;
        case 'Enter':
          onOpen(focusIndex);
          e.preventDefault();
          break;
        case ' ':
          onSelect(focusIndex);
          e.preventDefault();
          break;
        case 'Home':
          setFocusIndex(0);
          e.preventDefault();
          break;
        case 'End':
          setFocusIndex(itemCount - 1);
          e.preventDefault();
          break;
      }
    },
    [focusIndex, moveFocus, onOpen, onSelect, itemCount],
  );

  useEffect(() => {
    if (focusIndex !== null) {
      const element = document.querySelector(
        `[data-grid-index="${focusIndex}"]`,
      ) as HTMLElement | null;
      element?.focus();
    }
  }, [focusIndex]);

  return {
    focusIndex,
    containerRef,
    handleKeyDown,
    setFocusIndex,
  };
}
