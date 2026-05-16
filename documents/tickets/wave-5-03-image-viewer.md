# Wave 5.3 — Build Detail Viewer (Image Mode — Zoom/Pan)

| Field | Value |
|-------|-------|
| **Wave** | 5 — Frontend: Search, Detail View & Drag-and-Drop |
| **Seq** | 03 |
| **Estimate** | 3 hours |
| **Depends on** | 4.1 (API types) |
| **Parallel** | Yes — can run in parallel with 5.4 (video viewer) |

---

## Overview

Build the image detail viewer that displays the full-resolution image in a modal/overlay. Supports click-to-fit (image fits the viewport), scroll-to-zoom, and click-and-drag to pan. This is the primary way users inspect images.

## Prerequisites

- API types (4.1) — `MediaItemDetail` with `file_url`
- Image file serving endpoint (2.5) — backend must be running to serve full images
- `file_url` points to `/api/v1/media/:id/file`

## Reference Files

- `documents/plans/development-plan.md` — §10.Q8 (click-to-fit + scroll-to-zoom), §5 Wave 5 task 5.3
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/viewer/
├── image-viewer.tsx             # Image detail viewer with zoom/pan
└── __tests__/
    └── image-viewer.test.tsx    # Component tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Displays the full-resolution image via `file_url` (not the thumbnail)
- [ ] Image initially scales to **fit the viewport** (contained, not cropped)
- [ ] **Scroll to zoom**: mouse wheel zooms in/out centered on cursor position
- [ ] **Click and drag to pan**: when zoomed in, user can pan the image
- [ ] **Double-click to toggle fit/zoom**: double-click toggles between fit-to-screen and 100% zoom
- [ ] Zoom level indicator (optional but nice: "150%")
- [ ] Loading spinner while the full image is loading
- [ ] Error state if image fails to load ("Unable to load image")
- [ ] Keyboard: `+`/`-` or `Ctrl+=`/`Ctrl+-` for zoom; arrow keys for pan
- [ ] Close button (×) in the corner (handled by parent detail-view in 5.6)
- [ ] Background is dark/black to reduce glare around the image

## Implementation Notes

**Image viewer with zoom/pan:**
```tsx
import React, { useState, useCallback, useRef, useEffect } from 'react';
import type { MediaItemDetail } from '../../types/media';

interface ImageViewerProps {
  item: MediaItemDetail;
}

export function ImageViewer({ item }: ImageViewerProps) {
  const [zoom, setZoom] = useState(1);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const [imageLoaded, setImageLoaded] = useState(false);
  const [imageError, setImageError] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Fit to viewport on first load
  const [fitMode, setFitMode] = useState(true);

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setZoom((prev) => Math.max(0.1, Math.min(10, prev * delta)));
    setFitMode(false);
  }, []);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (zoom > 1) {
      setIsDragging(true);
      setDragStart({ x: e.clientX - position.x, y: e.clientY - position.y });
    }
  }, [zoom, position]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (isDragging) {
      setPosition({
        x: e.clientX - dragStart.x,
        y: e.clientY - dragStart.y,
      });
    }
  }, [isDragging, dragStart]);

  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  const handleDoubleClick = useCallback(() => {
    if (fitMode) {
      setFitMode(false);
      setZoom(1);
    } else {
      setFitMode(true);
      setZoom(1);
      setPosition({ x: 0, y: 0 });
    }
  }, [fitMode]);

  return (
    <div
      ref={containerRef}
      className="w-full h-full flex items-center justify-center bg-black/90 overflow-hidden cursor-grab active:cursor-grabbing"
      onWheel={handleWheel}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      onDoubleClick={handleDoubleClick}
    >
      {imageError ? (
        <div className="text-gray-400 text-center">
          <p className="text-lg">Unable to load image</p>
          <button onClick={() => { setImageError(false); setImageLoaded(false); }}
            className="mt-2 text-blue-400 hover:text-blue-300">Retry</button>
        </div>
      ) : (
        <>
          {!imageLoaded && (
            <div className="absolute inset-0 flex items-center justify-center">
              <div className="animate-spin h-8 w-8 border-2 border-blue-500 border-t-transparent rounded-full" />
            </div>
          )}
          <img
            src={item.file_url}
            alt={item.filename}
            onLoad={() => setImageLoaded(true)}
            onError={() => setImageError(true)}
            className={`select-none transition-opacity duration-200 ${
              imageLoaded ? 'opacity-100' : 'opacity-0'
            } ${fitMode ? 'max-w-full max-h-full object-contain' : ''}`}
            style={!fitMode ? {
              transform: `translate(${position.x}px, ${position.y}px) scale(${zoom})`,
              transformOrigin: 'center center',
            } : {}}
            draggable={false}
          />
        </>
      )}

      {/* Zoom indicator */}
      {!fitMode && zoom !== 1 && (
        <div className="absolute bottom-4 right-4 bg-black/70 text-white text-xs px-2 py-1 rounded">
          {Math.round(zoom * 100)}%
        </div>
      )}
    </div>
  );
}
```

**Alternative — use a library:** For production-quality zoom/pan with touch support, consider using a lightweight library like `react-zoom-pan-pinch` or building on top of CSS transforms. The above implementation covers the basics but may need polish for edge cases.

## Test Strategy

```tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { ImageViewer } from '../image-viewer';

const mockItem: MediaItemDetail = {
  id: '1', filename: 'test.png', path: '2025/test.png',
  mime_type: 'image/png',
  thumbnail_url: '/api/v1/media/1/thumbnail',
  file_url: '/api/v1/media/1/file',
  width: 896, height: 1216, file_size: 245760,
  created_at: '2025-01-01T00:00:00Z', modified_at: '2025-01-01T00:00:00Z',
  metadata: null,
};

describe('ImageViewer', () => {
  it('renders image with file_url as src', () => {
    render(<ImageViewer item={mockItem} />);
    const img = screen.getByRole('img');
    expect(img).toHaveAttribute('src', '/api/v1/media/1/file');
    expect(img).toHaveAttribute('alt', 'test.png');
  });

  it('shows loading state before image loads', () => {
    render(<ImageViewer item={mockItem} />);
    // Spinner should be present initially
  });

  it('responds to double-click for fit toggle', () => {
    render(<ImageViewer item={mockItem} />);
    const container = screen.getByRole('img').parentElement!;
    fireEvent.doubleClick(container);
    // Assert fit mode changed
  });
});
```
