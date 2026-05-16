# Wave 6.9 — Accessibility Audit and Fixes (ARIA, Focus, Contrast)

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 09 |
| **Estimate** | 2 hours |
| **Depends on** | All frontend components (Waves 4–6) |
| **Parallel** | Yes — can run in parallel with 6.10 |

---

## Overview

Perform an accessibility audit of the entire frontend and fix issues. Ensure keyboard navigability, screen reader compatibility, sufficient color contrast, and proper ARIA attributes. Target WCAG 2.1 AA compliance.

## Prerequisites

- All UI components implemented (Waves 4–6)
- ESLint with accessibility plugin (optional — `eslint-plugin-jsx-a11y`)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 6 task 6.9
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

Changes across multiple files — no new files expected. Fixes applied to existing components.

## Acceptance Criteria (Pass/Fail)

- [ ] All interactive elements are keyboard-focusable (`tabIndex`, `onKeyDown`)
- [ ] Focus order is logical (follows visual layout)
- [ ] Focus is visible at all times (no `outline: none` without replacement)
- [ ] All images have `alt` text (descriptive or empty for decorative)
- [ ] ARIA labels on all icon-only buttons and inputs
- [ ] Color contrast ratio ≥ 4.5:1 for normal text, ≥ 3:1 for large text
- [ ] No color-only information (use text/icons in addition to color)
- [ ] Screen reader announces dynamic content changes (live regions)
- [ ] `role` attributes correctly assigned (grid → `role="grid"`, cards → `role="button"`, etc.)
- [ ] Detail view modal traps focus correctly
- [ ] Shortcuts overlay traps focus correctly
- [ ] Loading states announced via `aria-busy` or `aria-live`

## Implementation Notes

**Key areas to audit:**

1. **ThumbnailCard:**
   - Already has `role="button"`, `tabIndex`, `aria-label`, `onKeyDown`
   - Verify: image `alt` text is filename
   - Verify: focus ring visible

2. **ThumbnailGrid:**
   - Add `role="grid"` and `aria-rowcount`, `aria-colcount`
   - Add `aria-label="Media gallery"`
   - Announce loading via `aria-busy="true"`

3. **SearchBar:**
   - Already has `aria-label`, `type="search"`
   - Add `aria-controls` pointing to grid ID
   - Announce results count via `aria-live="polite"`

4. **DetailView (modal):**
   - Trap focus inside modal
   - `role="dialog"` and `aria-modal="true"`
   - `aria-labelledby` pointing to filename
   - Restore focus on close

5. **ConfigPanel:**
   - `role="dialog"` and `aria-modal="true"`
   - Trap focus inside

6. **Color contrast:**
   - Check all text/background combos against dark theme
   - Gray-400 (`#9CA3AF`) on gray-900 (`#111827`) → ratio 5.9:1 ✓
   - Gray-500 (`#6B7280`) on gray-850 → check ratio
   - Blue links on dark bg → check ratio

**Focus trap utility:**
```typescript
function useFocusTrap(ref: React.RefObject<HTMLElement>, isActive: boolean) {
  useEffect(() => {
    if (!isActive || !ref.current) return;

    const element = ref.current;
    const focusableSelector = 'a[href], button, input, textarea, select, [tabindex]:not([tabindex="-1"])';
    
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;
      
      const focusableElements = element.querySelectorAll(focusableSelector);
      if (focusableElements.length === 0) return;
      
      const first = focusableElements[0] as HTMLElement;
      const last = focusableElements[focusableElements.length - 1] as HTMLElement;
      
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };

    element.addEventListener('keydown', handleKeyDown);
    return () => element.removeEventListener('keydown', handleKeyDown);
  }, [ref, isActive]);
}
```

**Live region for search results:**
```tsx
<div aria-live="polite" aria-atomic="true" className="sr-only">
  {viewMode === 'search' 
    ? `${totalCount} results for "${searchQuery}"` 
    : `Showing ${allItems.length} of ${totalCount} media items`}
</div>
```

## Test Strategy

- Manual: Tab through entire application, verify all elements reachable
- Manual: Use screen reader (VoiceOver/NVDA) to verify announcements
- Automated: Use `axe-core` or `@axe-core/react` for automated checks
- Automated: Chrome DevTools Lighthouse accessibility audit

```typescript
// If adding axe-core:
import { axe, toHaveNoViolations } from 'jest-axe';
expect.extend(toHaveNoViolations);

it('has no accessibility violations', async () => {
  const { container } = render(<App />);
  const results = await axe(container);
  expect(results).toHaveNoViolations();
});
```
