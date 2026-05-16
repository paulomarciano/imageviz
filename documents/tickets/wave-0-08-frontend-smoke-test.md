# Wave 0.8 — Write Frontend App Smoke Test

| Field | Value |
|-------|-------|
| **Wave** | 0 — Project Scaffolding & CI |
| **Seq** | 08 |
| **Estimate** | 30 minutes |
| **Depends on** | 0.3 (React frontend) |
| **Parallel** | Can run in parallel with 0.5, 0.6, 0.7 |

---

## Overview

Write a smoke test for the React frontend using Vitest + Testing Library. This verifies the frontend test infrastructure works and the App component renders without crashing.

## Prerequisites

- `frontend/src/App.tsx` exists (from 0.3)
- Vitest and Testing Library installed (from 0.3's `package.json`)

## Reference Files

- `documents/plans/development-plan.md` — §7.3 Frontend Testing (Vitest + Testing Library)
- `.opencode/context/core/standards/test-coverage.md` — AAA pattern

## Deliverables

```
frontend/src/
└── App.test.tsx                 # Smoke test for App component
```

## Acceptance Criteria (Pass/Fail)

- [ ] `npm test` (or `npx vitest run`) passes
- [ ] Test verifies App renders without crashing (at minimum)
- [ ] Test uses Testing Library's `render()` and `screen` queries
- [ ] Test file imports from `vitest` (`describe`, `it`, `expect`)

## Implementation Notes

**Minimal smoke test:**
```tsx
// frontend/src/App.test.tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';

describe('App', () => {
  it('renders without crashing', () => {
    render(<App />);
    expect(screen.getByText('ImageViz')).toBeInTheDocument();
  });
});
```

**vitest config** — add to `vite.config.ts` or create `vitest.config.ts`:
```typescript
/// <reference types="vitest/config" />
export default defineConfig({
  test: {
    globals: true,
    environment: 'jsdom',
  },
  // ... other config
});
```

**Setup** — you may need a `frontend/src/setup-tests.ts` that imports `@testing-library/jest-dom` for matchers like `toBeInTheDocument()`.

## Test Strategy

- `npx vitest run` — must exit 0
- This smoke test establishes the frontend testing pattern for all subsequent component/hook tests
