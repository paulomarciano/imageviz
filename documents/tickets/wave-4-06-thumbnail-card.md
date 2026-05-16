# Wave 4.6 — Build Thumbnail Card Component

| Field | Value |
|-------|-------|
| **Wave** | 4 — Frontend: Core Layout & Infinite Scroll |
| **Seq** | 06 |
| **Estimate** | 2 hours |
| **Depends on** | 4.1 (API types) |
| **Parallel** | Can run in parallel with 4.5 |

---

## Overview

Build the thumbnail card component that renders a single media item in the grid. Each card shows the thumbnail image, filename, and dimensions. Cards include loading skeleton states and error handling for broken thumbnails.

## Prerequisites

- API types (4.1) — `MediaItem` type
- Tailwind configured (0.3)

## Reference Files

- `documents/plans/development-plan.md` — §3.3 MediaItem (list view), §8.2 lazy loading, §8.3 useMemo/useCallback
- `.opencode/context/core/standards/code-quality.md` — React patterns (memo, pure components)

## Deliverables

```
frontend/src/components/media/
├── thumbnail-card.tsx           # Thumbnail card component
└── __tests__/
    └── thumbnail-card.test.tsx  # Component tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Renders thumbnail image via `<img src={thumbnail_url}>` with `loading="lazy"`
- [ ] Displays filename below/beside the thumbnail
- [ ] Displays dimensions (e.g., "896×1216") and file size (formatted)
- [ ] Shows **loading skeleton** (pulsing gray placeholder) while image loads
- [ ] Shows **broken thumbnail placeholder** (icon + "No preview") when image fails to load
- [ ] Image uses `object-cover` to crop to a consistent aspect ratio in the card
- [ ] Card has a hover effect (subtle scale or border highlight) indicating clickability
- [ ] Card is wrapped in `React.memo` to prevent unnecessary re-renders during virtual scrolling
- [ ] Accessible: `role="button"`, `tabIndex={0}`, `aria-label` with filename
- [ ] Emits `onClick` event for navigation to detail view (handled by parent)

## Implementation Notes

```tsx
import { memo, useState } from 'react';
import type { MediaItem } from '../../types/media';

interface ThumbnailCardProps {
  item: MediaItem;
  onClick: (item: MediaItem) => void;
}

function formatFileSize(bytes: number): string {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(1)} KB`;
  return `${bytes} B`;
}

export const ThumbnailCard = memo(function ThumbnailCard({ item, onClick }: ThumbnailCardProps) {
  const [imageLoaded, setImageLoaded] = useState(false);
  const [imageError, setImageError] = useState(false);

  return (
    <div
      role="button"
      tabIndex={0}
      aria-label={`View ${item.filename}`}
      className="group cursor-pointer rounded-lg overflow-hidden bg-gray-800 border border-gray-700 
                 hover:border-gray-500 hover:scale-[1.02] transition-all duration-150 focus:outline-none 
                 focus:ring-2 focus:ring-blue-500"
      onClick={() => onClick(item)}
      onKeyDown={(e) => { if (e.key === 'Enter') onClick(item); }}
    >
      {/* Image container */}
      <div className="relative aspect-[3/4] bg-gray-700 overflow-hidden">
        {!imageError ? (
          <>
            {/* Skeleton overlay */}
            {!imageLoaded && (
              <div className="absolute inset-0 bg-gray-700 animate-pulse" />
            )}
            <img
              src={item.thumbnail_url}
              alt={item.filename}
              loading="lazy"
              onLoad={() => setImageLoaded(true)}
              onError={() => setImageError(true)}
              className={`w-full h-full object-cover transition-opacity duration-300 ${
                imageLoaded ? 'opacity-100' : 'opacity-0'
              }`}
            />
          </>
        ) : (
          /* Broken thumbnail fallback */
          <div className="flex items-center justify-center h-full text-gray-500">
            <div className="text-center">
              <svg className="w-8 h-8 mx-auto mb-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5}
                  d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
              </svg>
              <p className="text-xs">No preview</p>
            </div>
          </div>
        )}
      </div>

      {/* Info bar */}
      <div className="p-2 text-xs">
        <p className="truncate text-gray-200 font-medium">{item.filename}</p>
        <p className="text-gray-400">
          {item.width && item.height ? `${item.width}×${item.height}` : 'Unknown'}
          <span className="mx-1">·</span>
          {formatFileSize(item.file_size)}
        </p>
      </div>
    </div>
  );
});
```

**Aspect ratio choice:**
The card uses `aspect-[3/4]` — this works well for ComfyUI outputs which are often vertical (portrait aspect ratio). For a vertical screen layout, the grid will be showing 3-4 items per row (per §10.Q1), so the card width is ~25-33% of the viewport.

**Memo:** `React.memo` is critical here — virtual scroll renders hundreds of cards, and without memo, every scroll event would re-render all visible cards.

## Test Strategy

```tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { ThumbnailCard } from '../thumbnail-card';

const mockItem: MediaItem = {
  id: '1', filename: 'test.png', path: '2025/test.png',
  mime_type: 'image/png', thumbnail_url: '/api/v1/media/1/thumbnail',
  width: 896, height: 1216, file_size: 245760,
  created_at: '2025-01-01T00:00:00Z', modified_at: '2025-01-01T00:00:00Z',
};

describe('ThumbnailCard', () => {
  it('renders filename and dimensions', () => {
    render(<ThumbnailCard item={mockItem} onClick={vi.fn()} />);
    expect(screen.getByText('test.png')).toBeInTheDocument();
    expect(screen.getByText(/896×1216/)).toBeInTheDocument();
    expect(screen.getByText(/245.8 KB/)).toBeInTheDocument();
  });

  it('calls onClick when clicked', async () => {
    const onClick = vi.fn();
    render(<ThumbnailCard item={mockItem} onClick={onClick} />);
    await fireEvent.click(screen.getByRole('button'));
    expect(onClick).toHaveBeenCalledWith(mockItem);
  });

  it('shows image with loading="lazy"', () => {
    render(<ThumbnailCard item={mockItem} onClick={vi.fn()} />);
    const img = screen.getByRole('img');
    expect(img).toHaveAttribute('loading', 'lazy');
  });

  it('shows placeholder when image fails', async () => {
    render(<ThumbnailCard item={mockItem} onClick={vi.fn()} />);
    const img = screen.getByRole('img');
    fireEvent.error(img);
    expect(await screen.findByText('No preview')).toBeInTheDocument();
  });
});
```
