# Wave 6.8 — Add Keyboard Shortcuts Panel

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 08 |
| **Estimate** | 1 hour |
| **Depends on** | 5.8 (keyboard nav), 5.3/5.4 (viewers), 5.6 (detail view) |
| **Parallel** | No |

---

## Overview

Add a keyboard shortcuts overlay that appears when the user presses `?`. Lists all available keyboard shortcuts with their descriptions. This helps users discover and learn power-user features.

## Prerequisites

- Keyboard navigation in grid (5.8)
- Detail view with keyboard controls (5.6)
- Image/video viewer keyboard shortcuts (5.3/5.4)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 6 task 6.8
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/shared/
├── shortcuts-panel.tsx          # Keyboard shortcuts overlay
└── __tests__/
    └── shortcuts-panel.test.tsx # Component tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Overlay appears when `?` key is pressed (no modifier needed: just `?` when no input is focused)
- [ ] Overlay closes on Escape or clicking outside
- [ ] Lists all shortcuts organized by context:
  - **Global**: `?` (show shortcuts), `Esc` (close panel/detail), `/` (focus search)
  - **Grid**: `↑↓←→` (navigate), `Enter` (open), `Space` (select)
  - **Detail view**: `←→` (next/previous item), `Esc` (close)
  - **Image viewer**: `+/-` (zoom), `Double-click` (fit toggle), `Drag` (pan)
  - **Video viewer**: `Space` (play/pause), `←→` (seek), `F` (fullscreen)
- [ ] Shortcuts are displayed with readable key names (e.g., "Escape" not "Esc")
- [ ] Styled as a semi-transparent overlay with a card in the center
- [ ] Accessible: focus trapped inside overlay, dismissable

## Implementation Notes

```tsx
import { useEffect, useState, useCallback, useRef } from 'react';

interface Shortcut {
  keys: string[];
  description: string;
}

const shortcuts: { category: string; items: Shortcut[] }[] = [
  {
    category: 'Global',
    items: [
      { keys: ['?'], description: 'Show keyboard shortcuts' },
      { keys: ['/'], description: 'Focus search bar' },
      { keys: ['Esc'], description: 'Close panel / detail view' },
    ],
  },
  {
    category: 'Grid',
    items: [
      { keys: ['↑', '↓', '←', '→'], description: 'Navigate between items' },
      { keys: ['Enter'], description: 'Open detail view' },
      { keys: ['Space'], description: 'Select item' },
      { keys: ['Home'], description: 'Jump to first item' },
      { keys: ['End'], description: 'Jump to last item' },
    ],
  },
  {
    category: 'Detail View',
    items: [
      { keys: ['←', '→'], description: 'Previous / Next item' },
      { keys: ['Esc'], description: 'Close detail view' },
    ],
  },
  {
    category: 'Image Viewer',
    items: [
      { keys: ['Scroll'], description: 'Zoom in / out' },
      { keys: ['Double-click'], description: 'Toggle fit / 100%' },
      { keys: ['Click + Drag'], description: 'Pan when zoomed' },
      { keys: ['+', '-'], description: 'Zoom in / out (keyboard)' },
    ],
  },
  {
    category: 'Video Viewer',
    items: [
      { keys: ['Space'], description: 'Play / Pause' },
      { keys: ['←', '→'], description: 'Seek backward / forward 5s' },
      { keys: ['F'], description: 'Toggle fullscreen' },
    ],
  },
];

export function ShortcutsPanel({ isOpen, onClose }: { isOpen: boolean; onClose: () => void }) {
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Open on ?
      if (e.key === '?' && !isInputFocused()) {
        e.preventDefault();
        isOpen ? onClose() : onClose(); // Toggle — but we need an onOpen too
        // Better: use a Jotai atom for isOpen
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 flex items-center justify-center"
      onClick={onClose}
    >
      <div
        ref={panelRef}
        className="bg-gray-850 border border-gray-700 rounded-lg shadow-2xl max-w-lg w-full mx-4 max-h-[80vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Keyboard shortcuts"
      >
        <div className="flex items-center justify-between p-4 border-b border-gray-700">
          <h2 className="text-lg font-semibold text-white">Keyboard Shortcuts</h2>
          <button onClick={onClose} className="text-gray-400 hover:text-white">✕</button>
        </div>

        <div className="p-4 space-y-6">
          {shortcuts.map(({ category, items }) => (
            <section key={category}>
              <h3 className="text-sm font-medium text-gray-300 mb-2">{category}</h3>
              <div className="space-y-1.5">
                {items.map(({ keys, description }) => (
                  <div key={description} className="flex items-center justify-between text-sm">
                    <span className="text-gray-400">{description}</span>
                    <div className="flex gap-1">
                      {keys.map((key) => (
                        <kbd key={key} className="px-2 py-0.5 bg-gray-700 border border-gray-600 rounded text-xs text-gray-200 font-mono">
                          {key}
                        </kbd>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>

        <div className="p-3 border-t border-gray-700 text-center">
          <p className="text-xs text-gray-500">Press <kbd className="px-1 bg-gray-700 rounded">Esc</kbd> to close</p>
        </div>
      </div>
    </div>
  );
}

function isInputFocused(): boolean {
  const tag = document.activeElement?.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
}
```

**Atom integration:**
```typescript
// In ui-atoms.ts
export const shortcutsPanelOpenAtom = atom<boolean>(false);
```

## Test Strategy

```tsx
it('renders all shortcut categories', () => {
  render(<ShortcutsPanel isOpen={true} onClose={vi.fn()} />);
  
  expect(screen.getByText('Global')).toBeInTheDocument();
  expect(screen.getByText('Grid')).toBeInTheDocument();
  expect(screen.getByText('Detail View')).toBeInTheDocument();
});

it('closes on Escape', () => {
  const onClose = vi.fn();
  render(<ShortcutsPanel isOpen={true} onClose={onClose} />);
  fireEvent.keyDown(document, { key: 'Escape' });
  // onClose should be called
});
```
