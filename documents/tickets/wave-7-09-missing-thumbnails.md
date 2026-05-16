# Wave 7.9 — Add Graceful Degradation for Missing Thumbnails

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 09 |
| **Estimate** | 30 minutes |
| **Depends on** | 4.6 (thumbnail card) |
| **Parallel** | Can run in parallel with other Wave 7 tasks |

---

## Overview

Enhance the ThumbnailCard component to gracefully handle missing or broken thumbnail images. Instead of showing a broken image icon or error, display a clean placeholder that indicates the thumbnail is unavailable while the rest of the UI remains functional.

## Prerequisites

- ThumbnailCard component (4.6)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 7 task 7.9
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

Changes to `frontend/src/components/media/thumbnail-card.tsx`

## Acceptance Criteria (Pass/Fail)

- [ ] If thumbnail image fails to load (404, network error), show a placeholder instead of a broken image
- [ ] Placeholder shows a generic media icon and the filename
- [ ] Placeholder has the same dimensions as a loaded thumbnail card (no layout shift)
- [ ] Placeholder uses muted colors (gray-700 background, gray-500 icon)
- [ ] Thumbnail card with placeholder is still clickable (opens detail view)
- [ ] Error is logged to console for debugging (not displayed to user)
- [ ] Retry mechanism (optional): "Click to reload" on the placeholder

## Implementation Notes

The thumbnail card (4.6) already has a basic error state. This task enhances it:

**Current error handling (from 4.6):**
```tsx
const [imageError, setImageError] = useState(false);

// In render:
{imageError ? (
  <div className="...">No preview</div>
) : (
  <img onError={() => setImageError(true)} ... />
)}
```

**Enhanced graceful degradation:**
```tsx
const [imageStatus, setImageStatus] = useState<'loading' | 'loaded' | 'error'>('loading');

// In render:
<div className="relative aspect-[3/4] bg-gray-700 overflow-hidden">
  {imageStatus === 'loading' && (
    <div className="absolute inset-0 bg-gray-700 animate-pulse" />
  )}
  
  {imageStatus === 'error' ? (
    /* Graceful placeholder */
    <div className="flex flex-col items-center justify-center h-full text-gray-500">
      {item.mime_type.startsWith('video/') ? (
        <svg className="w-10 h-10 mb-2" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5}
            d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" />
        </svg>
      ) : (
        <svg className="w-10 h-10 mb-2" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5}
            d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
      )}
      <p className="text-xs text-gray-500">{item.filename}</p>
      <p className="text-[10px] text-gray-600 mt-0.5">Preview unavailable</p>
    </div>
  ) : (
    <img
      src={item.thumbnail_url}
      alt={item.filename}
      loading="lazy"
      decoding="async"
      onLoad={() => setImageStatus('loaded')}
      onError={(e) => {
        console.warn('Thumbnail failed to load:', item.thumbnail_url);
        setImageStatus('error');
      }}
      className={`w-full h-full object-cover transition-opacity duration-300 ${
        imageStatus === 'loaded' ? 'opacity-100' : 'opacity-0'
      }`}
    />
  )}
</div>
```

**Key improvements:**
1. Different icon for video vs image (gives visual hint about file type)
2. Filename visible in the placeholder (identifies the file even without a thumbnail)
3. Console warning logs the failed URL (helps with debugging)
4. Placeholder has same `aspect-[3/4]` ratio → no layout shift
5. Card remains fully interactive (clickable, draggable)

## Test Strategy

```tsx
it('shows placeholder when thumbnail fails to load', () => {
  render(<ThumbnailCard item={mockItem} onClick={vi.fn()} />);
  
  const img = screen.getByRole('img');
  fireEvent.error(img);
  
  expect(screen.getByText('Preview unavailable')).toBeInTheDocument();
  // Card should still be clickable
  expect(screen.getByRole('button')).toBeInTheDocument();
});
```
