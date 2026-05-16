# Wave 5.8 — Implement Keyboard Navigation in Grid

| Field | Value |
|-------|-------|
| **Wave** | 5 — Frontend: Search, Detail View & Drag-and-Drop |
| **Seq** | 08 |
| **Estimate** | 1.5 hours |
| **Depends on** | 4.7 (thumbnail grid) |
| **Parallel** | No |

---

## Overview

Add keyboard navigation to the thumbnail grid. Arrow keys move focus between cards, Enter opens the detail view, and Space toggles selection. This is critical for accessibility and power-user workflows.

## Prerequisites

- Thumbnail grid (4.7)
- Detail view shell (5.6) — for Enter → open detail

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 5 task 5.8, §6 Wave 6 task 6.9 (accessibility)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/hooks/
├── use-keyboard-nav.ts          # Keyboard navigation hook
└── __tests__/
    └── use-keyboard-nav.test.ts # Hook tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Arrow keys move focus between thumbnail cards in the grid
- [ ] `←`/`→` moves left/right within the current row
- [ ] `↑`/`↓` moves up/down between rows
- [ ] Focus wraps at grid boundaries (leftmost → wraps to previous row rightmost)
- [ ] `Enter` opens the detail view for the focused item
- [ ] `Space` toggles selection of the focused item (for future multi-select)
- [ ] Focused item has a visible focus ring (blue outline)
- [ ] Focus is managed by a roving tabindex pattern (only one card has `tabIndex={0}` at a time)
- [ ] `Home` jumps to the first item in the grid
- [ ] `End` jumps to the last item in the grid
- [ ] Works with virtualized grid (only rendered items can receive focus)

## Implementation Notes

**Roving tabindex hook:**
```typescript
import { useCallback, useState, useRef, useEffect } from 'react';

interface UseKeyboardNavOptions {
  itemCount: number;
  columns: number;
  onSelect: (index: number) => void;
  onOpen: (index: number) => void;
}

export function useKeyboardNav({ itemCount, columns, onSelect, onOpen }: UseKeyboardNavOptions) {
  const [focusIndex, setFocusIndex] = useState<number | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const moveFocus = useCallback((current: number, direction: 'up' | 'down' | 'left' | 'right') => {
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
    
    // Scroll the focused item into view if needed
    const element = document.querySelector(`[data-grid-index="${next}"]`);
    element?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [itemCount, columns]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (focusIndex === null) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        setFocusIndex(0);
        e.preventDefault();
      }
      return;
    }

    switch (e.key) {
      case 'ArrowRight': moveFocus(focusIndex, 'right'); e.preventDefault(); break;
      case 'ArrowLeft': moveFocus(focusIndex, 'left'); e.preventDefault(); break;
      case 'ArrowDown': moveFocus(focusIndex, 'down'); e.preventDefault(); break;
      case 'ArrowUp': moveFocus(focusIndex, 'up'); e.preventDefault(); break;
      case 'Enter': onOpen(focusIndex); e.preventDefault(); break;
      case ' ': onSelect(focusIndex); e.preventDefault(); break;
      case 'Home': setFocusIndex(0); e.preventDefault(); break;
      case 'End': setFocusIndex(itemCount - 1); e.preventDefault(); break;
    }
  }, [focusIndex, moveFocus, onOpen, onSelect, itemCount]);

  // When focusIndex changes, focus the element
  useEffect(() => {
    if (focusIndex !== null) {
      const element = document.querySelector(`[data-grid-index="${focusIndex}"]`) as HTMLElement;
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
```

**Integration with ThumbnailCard:**
Add `data-grid-index` and roving tabindex:
```tsx
// In thumbnail-card.tsx
export const ThumbnailCard = memo(function ThumbnailCard({
  item,
  index,
  isFocused,
  onClick,
}: ThumbnailCardProps & { index: number; isFocused: boolean }) {
  return (
    <div
      role="button"
      tabIndex={isFocused ? 0 : -1}
      data-grid-index={index}
      onClick={() => onClick(item)}
      className={`... ${isFocused ? 'ring-2 ring-blue-500' : ''}`}
    >
      {/* ... */}
    </div>
  );
});
```

**Grid integration:**
```tsx
export function ThumbnailGrid({ onItemClick }: ThumbnailGridProps) {
  const columns = useResponsiveColumns(); // Hook to determine column count
  
  const { focusIndex, containerRef, handleKeyDown } = useKeyboardNav({
    itemCount: allItems.length,
    columns,
    onSelect: (index) => { /* future selection logic */ },
    onOpen: (index) => onItemClick(allItems[index]),
  });

  return (
    <div ref={containerRef} onKeyDown={handleKeyDown}>
      <VirtuosoGrid
        itemContent={(index) => (
          <DragSource item={allItems[index]}>
            <ThumbnailCard
              item={allItems[index]}
              index={index}
              isFocused={focusIndex === index}
              onClick={onItemClick}
            />
          </DragSource>
        )}
        // ...
      />
    </div>
  );
}
```

## Test Strategy

```typescript
import { renderHook, act } from '@testing-library/react';
import { useKeyboardNav } from '../use-keyboard-nav';

describe('useKeyboardNav', () => {
  it('starts with null focus index', () => {
    const { result } = renderHook(() =>
      useKeyboardNav({ itemCount: 10, columns: 4, onSelect: vi.fn(), onOpen: vi.fn() })
    );
    expect(result.current.focusIndex).toBeNull();
  });

  it('moves focus right on ArrowRight', () => {
    const { result } = renderHook(() =>
      useKeyboardNav({ itemCount: 10, columns: 4, onSelect: vi.fn(), onOpen: vi.fn() })
    );

    act(() => {
      result.current.setFocusIndex(0);
    });

    // Simulate keydown
    act(() => {
      result.current.handleKeyDown({ key: 'ArrowRight', preventDefault: vi.fn() } as any);
    });

    expect(result.current.focusIndex).toBe(1);
  });

  it('calls onOpen on Enter', () => {
    const onOpen = vi.fn();
    const { result } = renderHook(() =>
      useKeyboardNav({ itemCount: 10, columns: 4, onSelect: vi.fn(), onOpen })
    );

    act(() => { result.current.setFocusIndex(3); });
    act(() => {
      result.current.handleKeyDown({ key: 'Enter', preventDefault: vi.fn() } as any);
    });

    expect(onOpen).toHaveBeenCalledWith(3);
  });
});
```
