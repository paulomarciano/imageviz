/**
 * @vitest-environment jsdom
 *
 * Tests for useKeyboardNav — verifies arrow key navigation, Home/End
 * shortcuts, Enter/Space handlers, and roving tabindex focus management.
 */

import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { useKeyboardNav } from '../use-keyboard-nav';

describe('useKeyboardNav', () => {
  it('starts with null focus index', () => {
    // Arrange & Act
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    // Assert
    expect(result.current.focusIndex).toBeNull();
  });

  it('moves focus right on ArrowRight', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(0);
    });

    // Act
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowRight',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(result.current.focusIndex).toBe(1);
  });

  it('moves focus left on ArrowLeft', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(1);
    });

    // Act
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowLeft',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(result.current.focusIndex).toBe(0);
  });

  it('moves focus down on ArrowDown', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(0);
    });

    // Act
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowDown',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert — 0 + 4 columns = 4
    expect(result.current.focusIndex).toBe(4);
  });

  it('clamps down movement at the last item', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(8); // one row before last
    });

    // Act — 8 + 4 = 12, clamped to 9 (last item)
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowDown',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(result.current.focusIndex).toBe(9);
  });

  it('moves focus up on ArrowUp', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(4);
    });

    // Act — 4 - 4 = 0
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowUp',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(result.current.focusIndex).toBe(0);
  });

  it('clamps up movement at 0', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(3);
    });

    // Act — 3 - 4 = -1, clamped to 0
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowUp',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(result.current.focusIndex).toBe(0);
  });

  it('wraps right at the grid boundary', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(9);
    });

    // Act — (9 + 1) % 10 = 0
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowRight',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(result.current.focusIndex).toBe(0);
  });

  it('wraps left at the grid boundary', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(0);
    });

    // Act — (0 - 1 + 10) % 10 = 9
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowLeft',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(result.current.focusIndex).toBe(9);
  });

  it('moves focus on ArrowDown or ArrowUp when focusIndex is null', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    // Starts with null focus — ArrowDown should set focus to 0
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowDown',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    expect(result.current.focusIndex).toBe(0);

    act(() => {
      result.current.setFocusIndex(null);
    });

    // ArrowUp when null should also set focus to 0
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowUp',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    expect(result.current.focusIndex).toBe(0);
  });

  it('ignores arrow keys when focusIndex is null and key is not ArrowDown/ArrowUp', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    // Starts with null focus — ArrowRight should be ignored
    act(() => {
      result.current.handleKeyDown({
        key: 'ArrowRight',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    expect(result.current.focusIndex).toBeNull();
  });

  it('calls onOpen on Enter', () => {
    // Arrange
    const onOpen = vi.fn();
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen,
      }),
    );

    act(() => {
      result.current.setFocusIndex(3);
    });

    // Act
    act(() => {
      result.current.handleKeyDown({
        key: 'Enter',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(onOpen).toHaveBeenCalledWith(3);
  });

  it('calls onSelect on Space', () => {
    // Arrange
    const onSelect = vi.fn();
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect,
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(5);
    });

    // Act
    act(() => {
      result.current.handleKeyDown({
        key: ' ',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(onSelect).toHaveBeenCalledWith(5);
  });

  it('jumps to Home and End', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(5);
    });

    // Act — Home
    act(() => {
      result.current.handleKeyDown({
        key: 'Home',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(result.current.focusIndex).toBe(0);

    // Act — End
    act(() => {
      result.current.handleKeyDown({
        key: 'End',
        preventDefault: vi.fn(),
      } as unknown as React.KeyboardEvent);
    });

    // Assert
    expect(result.current.focusIndex).toBe(9);
  });

  it('prevents default on all handled keys', () => {
    // Arrange
    const { result } = renderHook(() =>
      useKeyboardNav({
        itemCount: 10,
        columns: 4,
        onSelect: vi.fn(),
        onOpen: vi.fn(),
      }),
    );

    act(() => {
      result.current.setFocusIndex(0);
    });

    // Act & Assert — verify each handled key calls preventDefault
    const keys = ['ArrowRight', 'ArrowLeft', 'ArrowDown', 'ArrowUp', 'Enter', ' ', 'Home', 'End'];
    for (const key of keys) {
      const preventDefault = vi.fn();
      act(() => {
        result.current.handleKeyDown({
          key,
          preventDefault,
        } as unknown as React.KeyboardEvent);
      });
      expect(preventDefault).toHaveBeenCalled();
    }
  });
});
