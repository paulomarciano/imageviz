/**
 * @vitest-environment jsdom
 *
 * Tests for useEscape — window-level Escape-to-close handler shared by the
 * config panel, shortcuts panel, and detail view. Verifies listener wiring,
 * the enabled gate, re-subscription on callback change, and cleanup.
 */

import { describe, it, expect, vi } from 'vitest';
import { renderHook, fireEvent } from '@testing-library/react';
import { useEscape } from '../use-escape';

describe('useEscape', () => {
  it('calls onClose when Escape is pressed on window', () => {
    // Arrange
    const onClose = vi.fn();
    renderHook(() => useEscape(onClose));

    // Act
    fireEvent.keyDown(window, { key: 'Escape' });

    // Assert
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('ignores other keys', () => {
    // Arrange
    const onClose = vi.fn();
    renderHook(() => useEscape(onClose));

    // Act
    fireEvent.keyDown(window, { key: 'Enter' });
    fireEvent.keyDown(window, { key: 'ArrowLeft' });

    // Assert
    expect(onClose).not.toHaveBeenCalled();
  });

  it('does nothing while disabled', () => {
    // Arrange
    const onClose = vi.fn();
    renderHook(() => useEscape(onClose, false));

    // Act
    fireEvent.keyDown(window, { key: 'Escape' });

    // Assert
    expect(onClose).not.toHaveBeenCalled();
  });

  it('stops calling onClose after unmount', () => {
    // Arrange
    const onClose = vi.fn();
    const { unmount } = renderHook(() => useEscape(onClose));
    unmount();

    // Act
    fireEvent.keyDown(window, { key: 'Escape' });

    // Assert
    expect(onClose).not.toHaveBeenCalled();
  });

  it('invokes the latest onClose after it changes', () => {
    // Arrange
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = renderHook(({ cb }: { cb: () => void }) => useEscape(cb), {
      initialProps: { cb: first },
    });

    // Act — swap the callback, then press Escape
    rerender({ cb: second });
    fireEvent.keyDown(window, { key: 'Escape' });

    // Assert
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });
});
