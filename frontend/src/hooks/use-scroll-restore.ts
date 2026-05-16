import { useCallback, useRef } from 'react';
import { useAtom } from 'jotai';
import { gridScrollIndexAtom } from '../store/ui-atoms';
import type { ListRange } from 'react-virtuoso';

export interface ScrollRestoreResult {
  /** The saved scroll index from localStorage, used to restore position. */
  savedIndex: number;
  /** Ref to the scroller DOM element for direct scroll access. */
  scrollerRef: React.MutableRefObject<HTMLElement | null>;
  /** Callback for VirtuosoGrid `rangeChanged` — persists the start index. */
  handleRangeChanged: (range: ListRange) => void;
  /** Callback for VirtuosoGrid `scrollerRef` — captures the scroll container. */
  handleScrollerRef: (ref: HTMLElement | null) => void;
}

/**
 * Persists and restores the grid scroll position using a Jotai atom backed by localStorage.
 *
 * Returns a `savedIndex` to pass as `initialTopMostItemIndex` and a
 * `handleRangeChanged` callback to save the current start index as the user scrolls.
 */
export function useScrollRestore(): ScrollRestoreResult {
  const [savedIndex, setSavedIndex] = useAtom(gridScrollIndexAtom);
  const scrollerRef = useRef<HTMLElement | null>(null);

  const handleRangeChanged = useCallback(
    (range: ListRange) => {
      setSavedIndex(range.startIndex);
    },
    [setSavedIndex],
  );

  const handleScrollerRef = useCallback((ref: HTMLElement | null) => {
    scrollerRef.current = ref;
  }, []);

  return {
    savedIndex,
    scrollerRef,
    handleRangeChanged,
    handleScrollerRef,
  };
}
