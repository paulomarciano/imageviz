# Wave 5.6 — Build Detail View Shell (Modal with Navigation)

| Field | Value |
|-------|-------|
| **Wave** | 5 — Frontend: Search, Detail View & Drag-and-Drop |
| **Seq** | 06 |
| **Estimate** | 2 hours |
| **Depends on** | 5.3 (image viewer), 5.4 (video viewer), 5.5 (metadata panel) |
| **Parallel** | No |

---

## Overview

Build the detail view shell — a modal/split view that hosts the image/video viewer and metadata panel. Provides navigation between items (← → arrows), close (Escape / × button), and handles both image and video content types.

## Prerequisites

- Image viewer (5.3)
- Video viewer (5.4)
- Metadata panel (5.5)
- Media items list for navigation context

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 5 task 5.6
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/viewer/
├── detail-view.tsx              # Detail view modal/shell
└── __tests__/
    └── detail-view.test.tsx     # Component tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Opens when a thumbnail card is clicked (or keyboard Enter)
- [ ] Displays the selected media item using the appropriate viewer (image or video)
- [ ] Metadata panel shown as a side panel (right side, resizable width ~300px)
- [ ] ← → arrow keys navigate to previous/next items in the current context (grid or search results)
- [ ] `Escape` key closes the detail view and returns to the grid
- [ ] Close button (×) in the top-right corner
- [ ] File info bar: filename, dimensions, file size, date
- [ ] Navigation buttons: ← Previous | Next →
- [ ] Scroll position restored in the grid when detail view closes (using Wave 4.9)
- [ ] Responsive: on narrow screens, metadata panel is hidden or toggled
- [ ] Transitions: fade in/out animation (150ms)

## Implementation Notes

```tsx
import { useCallback, useEffect } from 'react';
import { useAtom, useAtomValue } from 'jotai';
import { selectedMediaItemAtom, detailViewOpenAtom } from '../../store/media-atoms';
import { ImageViewer } from './image-viewer';
import { VideoViewer } from './video-viewer';
import { MetadataPanel } from './metadata-panel';

interface DetailViewProps {
  items: MediaItem[];  // Current context (grid or search results)
  currentIndex: number;
  onNavigate: (index: number) => void;
  onClose: () => void;
}

export function DetailView({ items, currentIndex, onNavigate, onClose }: DetailViewProps) {
  const item = items[currentIndex];
  if (!item) return null;

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      switch (e.key) {
        case 'Escape':
          onClose();
          break;
        case 'ArrowLeft':
          if (currentIndex > 0) onNavigate(currentIndex - 1);
          break;
        case 'ArrowRight':
          if (currentIndex < items.length - 1) onNavigate(currentIndex + 1);
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [currentIndex, items.length, onNavigate, onClose]);

  // Lock body scroll while detail view is open
  useEffect(() => {
    document.body.style.overflow = 'hidden';
    return () => { document.body.style.overflow = ''; };
  }, []);

  const isVideo = item.mime_type.startsWith('video/');

  return (
    <div className="fixed inset-0 z-50 bg-gray-900 flex animate-fadeIn">
      {/* Main viewer area */}
      <div className="flex-1 relative">
        {/* Viewer content */}
        {isVideo ? (
          <VideoViewer item={item as MediaItemDetail} />
        ) : (
          <ImageViewer item={item as MediaItemDetail} />
        )}

        {/* Close button */}
        <button
          onClick={onClose}
          className="absolute top-4 right-4 w-8 h-8 flex items-center justify-center rounded-full 
                     bg-black/50 hover:bg-black/70 text-white transition-colors"
          aria-label="Close detail view"
        >
          ✕
        </button>

        {/* File info bar */}
        <div className="absolute bottom-4 left-4 bg-black/70 text-white text-xs px-3 py-1.5 rounded">
          <span className="font-medium">{item.filename}</span>
          {item.width && item.height && (
            <span className="ml-2 text-gray-300">{item.width}×{item.height}</span>
          )}
          <span className="ml-2 text-gray-400">{formatFileSize(item.file_size)}</span>
        </div>

        {/* Navigation arrows */}
        {currentIndex > 0 && (
          <button
            onClick={() => onNavigate(currentIndex - 1)}
            className="absolute left-4 top-1/2 -translate-y-1/2 w-10 h-10 flex items-center justify-center 
                       rounded-full bg-black/50 hover:bg-black/70 text-white text-xl transition-colors"
            aria-label="Previous item"
          >
            ←
          </button>
        )}
        {currentIndex < items.length - 1 && (
          <button
            onClick={() => onNavigate(currentIndex + 1)}
            className="absolute right-4 top-1/2 -translate-y-1/2 w-10 h-10 flex items-center justify-center 
                       rounded-full bg-black/50 hover:bg-black/70 text-white text-xl transition-colors"
            aria-label="Next item"
          >
            →
          </button>
        )}
      </div>

      {/* Metadata side panel */}
      <div className="w-80 shrink-0 bg-gray-850 border-l border-gray-700 overflow-y-auto">
        <MetadataPanel metadata={(item as MediaItemDetail).metadata ?? null} />
      </div>
    </div>
  );
}

// Tailwind animation (add to tailwind config or use a CSS module):
// @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }
// .animate-fadeIn { animation: fadeIn 150ms ease-out; }
```

**Context about the items list:**
The `items` array comes from the current view context:
- If browsing: `allItems` from `useInfiniteMedia` hook
- If searching: `results` from `useSearch` hook

This is passed down from the parent component that wires the grid and detail view together.

**Note on `MediaItemDetail`:** The detail view needs the full media item (with `file_url` and `metadata`). Fetch this data when the detail view opens using `fetchMediaItem(id)` from the API client. Don't assume the grid's `MediaItem` has these fields.

## Test Strategy

```tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { DetailView } from '../detail-view';

const mockItems: MediaItemDetail[] = [
  { id: '1', filename: 'image.png', mime_type: 'image/png', file_url: '/api/v1/media/1/file', /* ... */ },
  { id: '2', filename: 'video.mp4', mime_type: 'video/mp4', file_url: '/api/v1/media/2/file', /* ... */ },
];

describe('DetailView', () => {
  it('calls onClose on Escape key', () => {
    const onClose = vi.fn();
    render(<DetailView items={mockItems} currentIndex={0} onNavigate={vi.fn()} onClose={onClose} />);
    
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('navigates with arrow keys', () => {
    const onNavigate = vi.fn();
    render(<DetailView items={mockItems} currentIndex={0} onNavigate={onNavigate} onClose={vi.fn()} />);
    
    fireEvent.keyDown(window, { key: 'ArrowRight' });
    expect(onNavigate).toHaveBeenCalledWith(1);
  });

  it('renders image viewer for images', () => {
    render(<DetailView items={mockItems} currentIndex={0} onNavigate={vi.fn()} onClose={vi.fn()} />);
    expect(screen.getByAltText('image.png')).toBeInTheDocument();
  });

  it('renders video viewer for videos', () => {
    render(<DetailView items={mockItems} currentIndex={1} onNavigate={vi.fn()} onClose={vi.fn()} />);
    // Video element should be present
  });
});
```
