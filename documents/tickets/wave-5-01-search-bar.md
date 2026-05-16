# Wave 5.1 — Build Search Bar Component with Debounced Input

| Field | Value |
|-------|-------|
| **Wave** | 5 — Frontend: Search, Detail View & Drag-and-Drop |
| **Seq** | 01 |
| **Estimate** | 1.5 hours |
| **Depends on** | 4.4 (useSearch hook) |
| **Parallel** | No |

---

## Overview

Build the search bar component with a text input that provides live filtering of the thumbnail grid. The input is debounced (300ms) so the search API is not called on every keystroke. Pressing Escape clears the search and returns to the full grid view.

## Prerequisites

- `useSearch` hook (4.4)
- App shell with header (4.5) — search bar typically lives in the header
- Jotai atoms for search state (5.2 — can be developed concurrently)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 5 task 5.1, §10.Q5 (free-text search)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/search/
├── search-bar.tsx               # Search input component
└── __tests__/
    └── search-bar.test.tsx      # Component tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Text input field with placeholder "Search media..."
- [ ] Input is debounced — search triggered 300ms after user stops typing
- [ ] Visual indicator (spinner or subtle pulse) while debouncing/searching
- [ ] Search results replace the grid view (wired via Jotai atoms in 5.2)
- [ ] Pressing `Escape` clears the search input
- [ ] Clear button (× icon) visible when input has text
- [ ] Input is autofocused when the app loads (or can be focused via `/` keyboard shortcut in Wave 6.8)
- [ ] Accessible: `aria-label`, `type="search"`, keyboard navigable
- [ ] Styled consistently with the dark theme

## Implementation Notes

```tsx
import { useState, useCallback, useRef, useEffect } from 'react';
import { useSetAtom } from 'jotai';
import { searchQueryAtom } from '../../store/search-atoms';

export function SearchBar() {
  const [localQuery, setLocalQuery] = useState('');
  const setSearchQuery = useSetAtom(searchQueryAtom);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout>>();

  // Debounce: update atom 300ms after last keystroke
  useEffect(() => {
    debounceRef.current = setTimeout(() => {
      setSearchQuery(localQuery);
    }, 300);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [localQuery, setSearchQuery]);

  const handleClear = useCallback(() => {
    setLocalQuery('');
    setSearchQuery('');
    inputRef.current?.focus();
  }, [setSearchQuery]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      handleClear();
    }
  }, [handleClear]);

  return (
    <div className="relative flex items-center">
      {/* Search icon */}
      <svg className="absolute left-3 w-4 h-4 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
          d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>

      <input
        ref={inputRef}
        type="search"
        aria-label="Search media"
        placeholder="Search media..."
        value={localQuery}
        onChange={(e) => setLocalQuery(e.target.value)}
        onKeyDown={handleKeyDown}
        className="w-64 pl-10 pr-8 py-1.5 bg-gray-700 border border-gray-600 rounded-md
                   text-sm text-white placeholder-gray-400
                   focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500
                   transition-colors"
      />

      {/* Clear button */}
      {localQuery && (
        <button
          onClick={handleClear}
          className="absolute right-2 p-0.5 rounded hover:bg-gray-600 text-gray-400 hover:text-white"
          aria-label="Clear search"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      )}
    </div>
  );
}
```

**Placement in header (header.tsx):**
```tsx
<header className="h-12 flex items-center justify-between px-4 bg-gray-800 border-b border-gray-700 shrink-0">
  <h1 className="text-lg font-semibold">ImageViz</h1>
  <SearchBar />   {/* Centered or right-aligned */}
  <div>{/* Future controls */}</div>
</header>
```

**Alternate — centralized layout with search in the middle:**
```
[ImageViz]     [   🔍 Search media...   ×   ]     [Settings]
```

## Test Strategy

```tsx
import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SearchBar } from '../search-bar';

describe('SearchBar', () => {
  it('renders search input', () => {
    render(<SearchBar />);
    expect(screen.getByPlaceholderText('Search media...')).toBeInTheDocument();
  });

  it('shows clear button when text is entered', async () => {
    render(<SearchBar />);
    const input = screen.getByPlaceholderText('Search media...');
    
    await userEvent.type(input, 'test');
    expect(screen.getByLabelText('Clear search')).toBeInTheDocument();
  });

  it('clears input on Escape key', async () => {
    render(<SearchBar />);
    const input = screen.getByPlaceholderText('Search media...');
    
    await userEvent.type(input, 'test');
    fireEvent.keyDown(input, { key: 'Escape' });
    
    expect(input).toHaveValue('');
  });

  it('clears input on clear button click', async () => {
    render(<SearchBar />);
    const input = screen.getByPlaceholderText('Search media...');
    
    await userEvent.type(input, 'test');
    await userEvent.click(screen.getByLabelText('Clear search'));
    
    expect(input).toHaveValue('');
  });
});
```
