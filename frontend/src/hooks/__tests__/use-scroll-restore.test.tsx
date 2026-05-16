/**
 * @vitest-environment jsdom
 *
 * Tests for useScrollRestore — verifies scroll position persistence via
 * Jotai atoms (backed by sessionStorage), ref capture, and range callbacks.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useScrollRestore } from '../use-scroll-restore';
import { Provider as JotaiProvider, createStore } from 'jotai';

/** Build a Jotai provider wrapper with a fresh store per test. */
function createWrapper() {
  const store = createStore();
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <JotaiProvider store={store}>{children}</JotaiProvider>;
  };
}

describe('useScrollRestore', () => {
  beforeEach(() => {
    // Clear sessionStorage so each test starts with a fresh atom default (0).
    sessionStorage.clear();
  });

  it('returns savedIndex, scrollerRef, handleRangeChanged, handleScrollerRef', () => {
    // Act
    const { result } = renderHook(() => useScrollRestore(), {
      wrapper: createWrapper(),
    });

    // Assert
    expect(result.current.savedIndex).toBe(0);
    expect(result.current.scrollerRef).toBeDefined();
    expect(result.current.handleRangeChanged).toBeInstanceOf(Function);
    expect(result.current.handleScrollerRef).toBeInstanceOf(Function);
  });

  it('saves index when handleRangeChanged is called', async () => {
    // Arrange
    const { result } = renderHook(() => useScrollRestore(), {
      wrapper: createWrapper(),
    });

    // Act — Jotai state updates are batched; use waitFor to flush
    result.current.handleRangeChanged({ startIndex: 42, endIndex: 52 });

    // Assert
    await waitFor(() => {
      expect(result.current.savedIndex).toBe(42);
    });
  });

  it('updates scrollerRef when handleScrollerRef is called', () => {
    // Arrange
    const { result } = renderHook(() => useScrollRestore(), {
      wrapper: createWrapper(),
    });

    // Act
    const mockElement = document.createElement('div');
    result.current.handleScrollerRef(mockElement);

    // Assert
    expect(result.current.scrollerRef.current).toBe(mockElement);
  });
});
