# Wave 4.8 — Implement Responsive Masonry Layout

| Field | Value |
|-------|-------|
| **Wave** | 4 — Frontend: Core Layout & Infinite Scroll |
| **Seq** | 08 |
| **Estimate** | 2 hours |
| **Depends on** | 4.7 (thumbnail grid) |
| **Parallel** | No |

---

## Overview

Enhance the thumbnail grid to use a responsive masonry-like layout that adapts to the window width. For a vertical screen (1080×1920), show 3-4 items per row. For horizontal (1920×1080), show 4-5. Items fill columns evenly with consistent gaps.

## Prerequisites

- Thumbnail grid with react-virtuoso (4.7)
- Tailwind responsive classes

## Reference Files

- `documents/plans/development-plan.md` — §10.Q1 (3-4 images per row on vertical 1080p), §10.Q10 (vertical + horizontal screens)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/media/
└── thumbnail-grid.tsx           # Updated: responsive column logic
```

## Acceptance Criteria (Pass/Fail)

- [ ] Grid columns increase with viewport width: 2 cols (<640px) → 3 cols (640-1024px) → 4 cols (1024-1280px) → 5 cols (1280px+)
- [ ] At 1080×1920 (vertical screen): grid shows 3 columns (since width is 1080px, which triggers `lg:` at 1024px — wait: the width is 1080px, so `lg:grid-cols-4` would apply. Let me check: at 1080px wide, standard Tailwind `lg:` is 1024px. So 4 columns. But the user wants 3-4. The `md:` breakpoint at 768px gives 3 columns, so at 1080px it would be md (3 cols) or lg (4 cols)? Let me re-examine.)

Actually, looking at the breakpoints:
- sm: 640px → 3 cols? No, that's too many for mobile. Let me use:
  - default (mobile): 2 cols
  - sm (640px): 3 cols
  - lg (1024px): 4 cols  
  - xl (1280px): 5 cols

At 1080px (vertical screen), the viewport is 1080×1920. The WIDTH is 1080px, which is ≥1024px (lg), so it triggers `lg:grid-cols-4`. That gives 4 columns. This matches "3-4 images per row."

For a horizontal screen at 1920×1080, the width is 1920px, which is ≥1280px (xl), so `xl:grid-cols-5` triggers. That gives 5 columns.

- [ ] At 1920×1080 (horizontal): grid shows 5 columns
- [ ] Items maintain aspect ratio (no stretching/squishing)
- [ ] Gap between items is consistent (12px / `gap-3`)
- [ ] Grid padding is consistent (12px / `p-3`)
- [ ] Columns resize smoothly on window resize (CSS-only, no JS)
- [ ] react-virtuoso correctly measures item heights in the responsive grid

## Implementation Notes

The responsive column logic is already included in the Wave 4.7 grid. This task specifically validates and refines it.

**Current implementation (from 4.7):**
```tsx
function ListContainer({ children, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      {...props}
      className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3 p-3"
    >
      {children}
    </div>
  );
}
```

**Adding masonry feel:** Since items have varying heights (different aspect ratios), a pure CSS grid results in each row having the same height (the tallest item dictates the row height). To get a true masonry layout where items pack tightly regardless of height, you'd need:

1. **CSS columns** — `column-count` property (true masonry, but order is top-to-bottom then left-to-right, which conflicts with virtual scroll's row-based rendering)
2. **react-virtuoso masonry** — react-virtuoso v4 has built-in masonry support via `Masonry` component
3. **Accept the grid row height** — Items with `aspect-[3/4]` will naturally have different computed heights, but CSS grid rows align them

**Recommendation:** Use react-virtuoso's built-in masonry support if available and performant, or accept the CSS grid behavior (uniform row heights, zero-waste layout with fixed aspect ratio cards). For ComfyUI images (mostly vertical 896×1216), the aspect ratio is consistent, so CSS grid works well.

**react-virtuoso MasonryGrid (if using):**
```tsx
import { MasonryGrid } from 'react-virtuoso';

// MasonryGrid auto-arranges items in columns based on available width
<MasonryGrid
  items={allItems}
  itemContent={(index) => <ThumbnailCard item={allItems[index]} onClick={onItemClick} />}
  columns={4}
  // ...
/>
```

**Verification at target resolutions:**
- 1080×1920: 1080 / 4 = 270px per column (minus gaps) — good for 200-300px thumbnails
- 1920×1080: 1920 / 5 = 384px per column — generous for thumbnails

## Test Strategy

- Manual verification: resize browser window, observe column count changes
- Visual regression test: screenshot at 1080p vertical and 1920×1080
- Unit test: verify Tailwind classes are applied at correct breakpoints (hard to test in jsdom)

```typescript
// Verify the grid has responsive classes
it('applies responsive grid classes', () => {
  const { container } = render(<ThumbnailGrid onItemClick={vi.fn()} />);
  const grid = container.querySelector('.grid');
  expect(grid).toHaveClass('grid-cols-2', 'sm:grid-cols-3', 'lg:grid-cols-4');
});
```
