# Wave 6.7 — Add Loading Skeletons (Grid, Detail, Config)

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 07 |
| **Estimate** | 1 hour |
| **Depends on** | None (independent shared component) |
| **Parallel** | Yes — can run in parallel with other Wave 6 tasks |

---

## Overview

Build skeleton loading components for the grid, detail view, and config panel. Skeletons provide visual feedback during data loading, preventing layout shift and giving users a sense of progress.

## Prerequisites

- Tailwind configured (0.3)
- Thumbnail grid (4.7), detail view (5.6), config panel (6.3)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 6 task 6.7
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/media/
├── skeleton-grid.tsx            # Grid loading skeleton (grid of pulsing cards)
└── skeleton-card.tsx            # Single card skeleton

frontend/src/components/shared/
└── skeleton.tsx                 # Generic skeleton primitive (reusable)
```

## Acceptance Criteria (Pass/Fail)

- [ ] `SkeletonGrid` renders a grid of pulsing placeholder cards (3-5 columns × 3-4 rows)
- [ ] Skeleton cards have the same dimensions as real ThumbnailCards (consistent layout)
- [ ] Animation: subtle pulse/breathing effect (`animate-pulse` from Tailwind)
- [ ] Color: `bg-gray-700` base, `bg-gray-600` during pulse
- [ ] `SkeletonDetail` renders placeholder for the viewer area + metadata panel
- [ ] `SkeletonConfig` renders placeholder for the config form fields
- [ ] Accessible: `aria-busy="true"`, `role="status"` or `aria-label="Loading"`
- [ ] Generic `Skeleton` component with `width`, `height`, `rounded` props for one-off uses

## Implementation Notes

**Generic skeleton:**
```tsx
interface SkeletonProps {
  className?: string;
}

export function Skeleton({ className = '' }: SkeletonProps) {
  return (
    <div
      role="status"
      aria-label="Loading"
      className={`bg-gray-700 animate-pulse rounded ${className}`}
    />
  );
}
```

**SkeletonCard (matches ThumbnailCard dimensions):**
```tsx
export function SkeletonCard() {
  return (
    <div className="rounded-lg overflow-hidden bg-gray-800 border border-gray-700" aria-busy="true">
      {/* Image placeholder */}
      <div className="aspect-[3/4] bg-gray-700 animate-pulse" />
      {/* Info placeholder */}
      <div className="p-2 space-y-2">
        <Skeleton className="h-3 w-3/4" />
        <Skeleton className="h-2 w-1/2" />
      </div>
    </div>
  );
}
```

**SkeletonGrid (grid of skeleton cards):**
```tsx
export function SkeletonGrid() {
  return (
    <div aria-busy="true" aria-label="Loading media" className="p-3">
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3">
        {Array.from({ length: 15 }, (_, i) => (
          <SkeletonCard key={i} />
        ))}
      </div>
    </div>
  );
}
```

**Usage in grid:**
```tsx
if (isLoading && !allItems.length) {
  return <SkeletonGrid />;
}
```

**Note:** The `!allItems.length` check ensures skeleton only shows on initial load, not during pagination. During `fetchNextPage`, show a smaller loader at the bottom.

## Test Strategy

```tsx
it('renders correct number of skeleton cards', () => {
  const { container } = render(<SkeletonGrid />);
  const cards = container.querySelectorAll('[aria-busy="true"]');
  // Grid has 1 parent aria-busy + 15 card aria-busy
  expect(cards.length).toBeGreaterThanOrEqual(15);
});
```
