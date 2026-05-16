# Wave 4.5 — Build Application Shell Layout

| Field | Value |
|-------|-------|
| **Wave** | 4 — Frontend: Core Layout & Infinite Scroll |
| **Seq** | 05 |
| **Estimate** | 2 hours |
| **Depends on** | None (independent component) |
| **Parallel** | Can run in parallel with 4.3, 4.4 |

---

## Overview

Build the minimalistic application shell layout: a header bar with the app title and controls, and a main content area that holds the thumbnail grid. This establishes the visual frame for all frontend work. The design is desktop-first, optimized for both vertical (portrait) and horizontal (landscape) screen orientations.

## Prerequisites

- Tailwind CSS configured (0.3)
- React app scaffolded (0.3)

## Reference Files

- `documents/plans/development-plan.md` — §10.Q10 (desktop-only, vertical + horizontal screens), §12 project structure
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
frontend/src/components/layout/
├── app-shell.tsx                # Main layout container
└── header.tsx                   # Header bar component
```

## Acceptance Criteria (Pass/Fail)

- [ ] `AppShell` renders header + main content area using CSS grid or flexbox
- [ ] Layout fills the viewport: 100vw × 100vh, no scrollbars on the shell
- [ ] Header is fixed height (~48px), main content fills remaining space with overflow scroll
- [ ] Header displays "ImageViz" title (left) and placeholder for future controls (right)
- [ ] Dark theme: background `gray-900`, text `white`, header `gray-800`
- [ ] Minimalistic — no visual clutter, only essential elements
- [ ] Responsive: works on 1080p vertical screen (1080×1920) and standard horizontal (1920×1080)
- [ ] Content area uses CSS `overflow-y: auto` (the grid itself handles overflow via react-virtuoso)

## Implementation Notes

**app-shell.tsx:**
```tsx
import { Header } from './header';

interface AppShellProps {
  children: React.ReactNode;
}

export function AppShell({ children }: AppShellProps) {
  return (
    <div className="h-screen w-screen flex flex-col bg-gray-900 text-white overflow-hidden">
      <Header />
      <main className="flex-1 overflow-hidden">
        {children}
      </main>
    </div>
  );
}
```

**header.tsx:**
```tsx
export function Header() {
  return (
    <header className="h-12 flex items-center justify-between px-4 bg-gray-800 border-b border-gray-700 shrink-0">
      <div className="flex items-center gap-3">
        <h1 className="text-lg font-semibold tracking-tight">ImageViz</h1>
      </div>
      <div className="flex items-center gap-2">
        {/* Placeholder for search bar (Wave 5.1) */}
        {/* Placeholder for config button (Wave 6.3) */}
      </div>
    </header>
  );
}
```

**Usage in App.tsx:**
```tsx
function App() {
  return (
    <AppShell>
      {/* Thumbnail grid will go here (Wave 4.7) */}
      <div className="h-full flex items-center justify-center text-gray-500">
        <p>Configure watched folders to start viewing media</p>
      </div>
    </AppShell>
  );
}
```

**Design decisions:**
- Dark theme from the start — image browsing benefits from dark backgrounds (less eye strain, images pop more)
- `overflow-hidden` on shell → prevents double scrollbars (shell doesn't scroll, content area scrolls internally via react-virtuoso)
- Fixed header height `h-12` (48px) — enough for title + controls, not too tall

## Test Strategy

Component test:
```tsx
import { render, screen } from '@testing-library/react';
import { AppShell } from '../app-shell';

describe('AppShell', () => {
  it('renders header with title', () => {
    render(<AppShell><p>Content</p></AppShell>);
    expect(screen.getByText('ImageViz')).toBeInTheDocument();
    expect(screen.getByText('Content')).toBeInTheDocument();
  });

  it('renders children in main area', () => {
    render(<AppShell><div data-testid="child">test</div></AppShell>);
    expect(screen.getByTestId('child')).toBeInTheDocument();
  });
});
```
