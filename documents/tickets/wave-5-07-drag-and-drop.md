# Wave 5.7 — Implement External Drag-and-Drop (react-dnd)

| Field | Value |
|-------|-------|
| **Wave** | 5 — Frontend: Search, Detail View & Drag-and-Drop |
| **Seq** | 07 |
| **Estimate** | 2 hours |
| **Depends on** | 4.6 (thumbnail card) |
| **Parallel** | No |

---

## Overview

Implement OS-level drag-and-drop so users can drag thumbnails from the ImageViz grid directly into external applications (file explorer, image editor, Discord, etc.). Uses `react-dnd` with the HTML5 backend — the only library that supports native external drag operations.

## Prerequisites

- `react-dnd` ^16.0 and `react-dnd-html5-backend` ^16.0 installed (from 0.3)
- Thumbnail card component (4.6)
- File serving endpoint (2.5) — the file_url is used as the drag payload

## Reference Files

- `documents/plans/development-plan.md` — §2 Tech Stack (react-dnd for OS-level drag), §5 Wave 5 task 5.7
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/media/
├── drag-source.tsx              # Draggable wrapper around ThumbnailCard
└── (thumbnail-card.tsx — updated if needed)
```

## Acceptance Criteria (Pass/Fail)

- [ ] Thumbnail cards are draggable (initiate drag on mousedown + move)
- [ ] Drag preview shows the thumbnail image (not a generic file icon)
- [ ] Dropping the file in a file explorer copies the file to that location
- [ ] Dropping in an image editor (e.g., Photoshop) opens the file
- [ ] Dropping in a browser-based app (Discord web, etc.) uploads the file
- [ ] Drag payload includes the file URL and filename
- [ ] Drag doesn't interfere with click (click still opens detail view)
- [ ] Drag doesn't interfere with scroll (scroll on the grid still works)
- [ ] Visual feedback: dragged card shows reduced opacity during drag

## Implementation Notes

**Drag source wrapper:**
```tsx
import { useDrag } from 'react-dnd';
import { getEmptyImage } from 'react-dnd-html5-backend';
import { useEffect, useRef } from 'react';
import type { MediaItem } from '../../types/media';

interface DragSourceProps {
  item: MediaItem;
  children: React.ReactNode;
}

export const DRAG_TYPE = 'MEDIA_ITEM';

export function DragSource({ item, children }: DragSourceProps) {
  const previewRef = useRef<HTMLDivElement>(null);

  const [{ isDragging }, drag, preview] = useDrag(() => ({
    type: DRAG_TYPE,
    item: () => ({
      type: DRAG_TYPE,
      id: item.id,
      filename: item.filename,
      fileUrl: `/api/v1/media/${item.id}/file`,
      thumbnailUrl: item.thumbnail_url,
      mimeType: item.mime_type,
    }),
    collect: (monitor) => ({
      isDragging: monitor.isDragging(),
    }),
    end: (draggedItem, monitor) => {
      // Optional: track analytics or cleanup
    },
  }), [item]);

  // Use a custom drag preview image (shows the thumbnail)
  useEffect(() => {
    // Create a custom drag image from the thumbnail
    const img = new Image();
    img.src = item.thumbnail_url;
    img.onload = () => {
      // This sets the drag ghost image to the thumbnail
      // (Only works if we have a DOM element ref)
    };
  }, [item.thumbnail_url]);

  return (
    <div
      ref={drag}
      style={{ opacity: isDragging ? 0.4 : 1 }}
      className="cursor-grab active:cursor-grabbing"
    >
      {children}
    </div>
  );
}
```

**DnD Provider setup (in App.tsx or main.tsx):**
```tsx
import { DndProvider } from 'react-dnd';
import { HTML5Backend } from 'react-dnd-html5-backend';

function App() {
  return (
    <DndProvider backend={HTML5Backend}>
      {/* ... app content ... */}
    </DndProvider>
  );
}
```

**Integration with ThumbnailCard:**
Wrap each card in `DragSource`:
```tsx
// In thumbnail-grid.tsx
import { DragSource } from './drag-source';

<VirtuosoGrid
  itemContent={(index) => (
    <DragSource item={allItems[index]}>
      <ThumbnailCard
        item={allItems[index]}
        onClick={onItemClick}
      />
    </DragSource>
  )}
/>
```

**Native file drag payload:**
For OS-level drag to work, the drag data must include a URL that the target application can resolve. The HTML5 drag API supports `text/uri-list` for URLs:

```tsx
// Enhance the useDrag item to include native drag data
const [{ isDragging }, drag, preview] = useDrag(() => ({
  type: DRAG_TYPE,
  item: () => {
    // Set native drag data for OS-level drops
    // This is done in the dragstart event
    return { id: item.id, filename: item.filename };
  },
  options: {
    // Enable native drag effects
    dropEffect: 'copy',
  },
  collect: (monitor) => ({
    isDragging: monitor.isDragging(),
  }),
}), [item]);

// Use a useEffect to set the drag data on the element
useEffect(() => {
  const el = previewRef.current;
  if (!el) return;

  const handleDragStart = (e: DragEvent) => {
    // Set the URL as drag data for external applications
    const fileUrl = `http://localhost:3001/api/v1/media/${item.id}/file`;
    e.dataTransfer?.setData('text/uri-list', fileUrl);
    e.dataTransfer?.setData('text/plain', fileUrl);
    e.dataTransfer?.effectAllowed = 'copy';
    
    // Set a custom drag image
    const img = new Image();
    img.src = item.thumbnail_url;
    e.dataTransfer?.setDragImage(img, 50, 50);
  };

  el.addEventListener('dragstart', handleDragStart);
  return () => el.removeEventListener('dragstart', handleDragStart);
}, [item]);
```

**Important note on `react-dnd` and native drag:**
`react-dnd` abstracts the HTML5 drag API, but for OS-level file drops, the receiving application needs a file path or URL it can access. Since ImageViz serves files via HTTP, the drag payload is a URL (`http://localhost:3001/api/v1/media/{id}/file`). External applications that support URL drops (file explorers, browsers) will download the file.

**Limitations:**
- Not all applications support URL drops (e.g., basic text editors)
- The file must be accessible at the URL (localhost works for local apps, not for remote)
- For full file-system-level drag, you'd need to write a temporary file or use a custom protocol

## Test Strategy

Testing drag-and-drop in jsdom is limited. Focus on:
- Verifying the `useDrag` hook is called with correct config
- Verifying the drag source wraps the card
- Verifying opacity changes during drag state

```tsx
// Use react-dnd test backend for testing
import { TestBackend } from 'react-dnd-test-backend';
import { DndProvider } from 'react-dnd';

describe('DragSource', () => {
  it('renders children with drag ref', () => {
    // Test with TestBackend
  });
});
```

## External Docs

Use **ExternalScout** to fetch current react-dnd v16 docs for:
- `useDrag` hook API — item type, collect, end callback
- HTML5 backend setup
- Native drag data (`dataTransfer.setData`)
- Custom drag preview images
