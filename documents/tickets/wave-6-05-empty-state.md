# Wave 6.5 — Build Empty State Component

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 05 |
| **Estimate** | 45 minutes |
| **Depends on** | None (independent shared component) |
| **Parallel** | Yes — can run in parallel with any Wave 6 task |

---

## Overview

Build a reusable empty state component shown when there's no data to display (no watched folders configured, no files match search, etc.). Provides clear, actionable guidance to the user.

## Prerequisites

- Tailwind configured (0.3)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 6 task 6.5
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/shared/
└── empty-state.tsx              # Empty state component
```

## Acceptance Criteria (Pass/Fail)

- [ ] Renders an icon (optional), message, and optional action button
- [ ] Props: `message: string`, `action?: { label: string; onClick: () => void }`
- [ ] Uses dark theme colors (gray-500 text, gray-800 background area)
- [ ] Centered vertically and horizontally in parent container
- [ ] Accessible: message is visible, button is keyboard-focusable
- [ ] Used by: grid (no media), search (no results), detail view (no metadata)

## Implementation Notes

```tsx
interface EmptyStateProps {
  message: string;
  description?: string;
  icon?: React.ReactNode;
  action?: {
    label: string;
    onClick: () => void;
  };
}

export function EmptyState({ message, description, icon, action }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center h-full text-center p-8">
      {icon && (
        <div className="mb-4 text-gray-600">
          {icon}
        </div>
      )}
      
      <p className="text-gray-400 text-lg font-medium mb-1">{message}</p>
      
      {description && (
        <p className="text-gray-500 text-sm max-w-md">{description}</p>
      )}
      
      {action && (
        <button
          onClick={action.onClick}
          className="mt-4 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded 
                     transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
```

**Usage examples:**

Grid — no media indexed:
```tsx
<EmptyState
  message="No media found"
  description="Configure watched folders in Settings to start browsing your images and videos."
  icon={<FolderIcon className="w-12 h-12" />}
  action={{ label: 'Open Settings', onClick: () => setConfigOpen(true) }}
/>
```

Search — no results:
```tsx
<EmptyState
  message="No results for 'zzzzzzz'"
  description="Try a different search term."
/>
```

No metadata:
```tsx
<EmptyState
  message="No metadata available"
  description="This file doesn't contain extractable metadata (e.g., ComfyUI prompt data)."
/>
```
