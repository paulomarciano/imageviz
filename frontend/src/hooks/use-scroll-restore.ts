import { useCallback, useRef } from 'react';
import { useAtom } from 'jotai';
import { gridScrollIndexAtom } from '../store/ui-atoms';
import type { ListRange } from 'react-virtuoso';

/** Minimum ms between sessionStorage writes to avoid jank on rapid scroll. */
const THROTTLE_MS = 100;

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
 *
 * Performance: `handleRangeChanged` is throttled to every 100ms so rapid scroll
 * doesn't write to sessionStorage on every VirtuosoGrid range event.
 */
export function useScrollRestore(): ScrollRestoreResult {
  const [savedIndex, setSavedIndex] = useAtom(gridScrollIndexAtom);
  const scrollerRef = useRef<HTMLElement | null>(null);
  const lastWriteRef = useRef(0);

  const handleRangeChanged = useCallback(
    (range: ListRange) => {
      const now = Date.now();
      if (now - lastWriteRef.current >= THROTTLE_MS) {
        lastWriteRef.current = now;
        setSavedIndex(range.startIndex);
      }
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
