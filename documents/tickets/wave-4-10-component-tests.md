# Wave 4.10 — Write Component Tests (Grid, Card, Hooks)

| Field | Value |
|-------|-------|
| **Wave** | 4 — Frontend: Core Layout & Infinite Scroll |
| **Seq** | 10 |
| **Estimate** | 2 hours |
| **Depends on** | 4.3–4.9 (all Wave 4 components and hooks) |
| **Parallel** | No (verifies entire Wave 4) |

---

## Overview

Write comprehensive component and hook tests for all Wave 4 deliverables. Tests use Vitest + Testing Library with MSW (Mock Service Worker) for API mocking. Verify the grid renders correctly, infinite scroll works, card states display properly, and hooks behave as expected.

## Prerequisites

- All Wave 4 components and hooks implemented (4.1–4.9)
- Vitest + Testing Library configured (0.3)
- MSW installed for API mocking

## Reference Files

- `documents/plans/development-plan.md` — §7.3 Frontend Testing (Vitest + Testing Library + MSW), §7.5 Test Data Strategy
- `.opencode/context/core/standards/test-coverage.md` — AAA pattern
- `frontend/src/test-utils/` — test render helpers

## Deliverables

```
frontend/src/
├── test-utils/
│   ├── msw-handlers.ts           # MSW request handlers
│   └── render-utils.tsx          # Test render with providers (QueryClient, etc.)
├── components/media/__tests__/
│   ├── thumbnail-card.test.tsx   # Card tests (already written in 4.6)
│   └── thumbnail-grid.test.tsx   # Grid tests
└── hooks/__tests__/
    ├── use-infinite-media.test.ts # Hook tests (already written in 4.3)
    ├── use-search.test.ts        # Hook tests (already written in 4.4)
    └── use-scroll-restore.test.ts # Hook tests (already written in 4.9)
```

## Acceptance Criteria (Pass/Fail)

- [ ] `npm test` (or `npx vitest run`) passes with all Wave 4 tests
- [ ] Grid test: renders multiple thumbnail cards when data is loaded
- [ ] Grid test: shows loading skeleton when `isLoading` is true
- [ ] Grid test: shows error state with retry button when `isError` is true
- [ ] Grid test: shows empty state when no items
- [ ] Grid test: `endReached` triggers `fetchNextPage`
- [ ] Grid test: `onItemClick` called when a card is clicked
- [ ] Card tests (from 4.6): all pass
- [ ] Hook tests (from 4.3, 4.4, 4.9): all pass
- [ ] All tests use AAA pattern
- [ ] No actual network calls — all mocked via MSW or vi.mock

## Implementation Notes

**MSW handlers:**
```typescript
// frontend/src/test-utils/msw-handlers.ts
import { http, HttpResponse } from 'msw';

export const handlers = [
  http.get('/api/v1/media', ({ request }) => {
    const url = new URL(request.url);
    const limit = parseInt(url.searchParams.get('limit') ?? '100');
    const cursor = url.searchParams.get('cursor');
    
    // Generate mock media items
    const items = Array.from({ length: limit }, (_, i) => ({
      id: `mock-id-${i}`,
      filename: `image_${i}.png`,
      path: `2025/image_${i}.png`,
      mime_type: 'image/png',
      thumbnail_url: `/api/v1/media/mock-id-${i}/thumbnail`,
      width: 896,
      height: 1216,
      file_size: 245760,
      created_at: '2025-01-01T00:00:00Z',
      modified_at: '2025-01-01T00:00:00Z',
    }));

    return HttpResponse.json({
      data: items,
      meta: {
        next_cursor: cursor ? null : '2025-01-01T00:00:00Z',
        next_cursor_id: cursor ? null : `mock-id-${limit - 1}`,
        has_more: !cursor,  // First page has more, second page doesn't
        total: 250,
      },
    });
  }),

  http.get('/api/v1/search', ({ request }) => {
    const url = new URL(request.url);
    const q = url.searchParams.get('q') ?? '';
    return HttpResponse.json({
      data: [
        { id: 'search-1', filename: `${q}_result.png`, /* ... */ },
      ],
      meta: {
        next_cursor: null, next_cursor_id: null, has_more: false,
        total: 1, query: q,
      },
    });
  }),

  http.get('/api/v1/health', () => {
    return HttpResponse.json({ status: 'ok', version: '0.1.0' });
  }),
];
```

**Test render utility:**
```typescript
// frontend/src/test-utils/render-utils.tsx
import { render as rtlRender } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Provider as JotaiProvider, createStore } from 'jotai';

interface RenderOptions {
  queryClient?: QueryClient;
}

export function renderWithProviders(
  ui: React.ReactElement,
  options?: RenderOptions,
) {
  const queryClient = options?.queryClient ?? new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const jotaiStore = createStore();

  function Wrapper({ children }: { children: React.ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <JotaiProvider store={jotaiStore}>
          {children}
        </JotaiProvider>
      </QueryClientProvider>
    );
  }

  return rtlRender(ui, { wrapper: Wrapper });
}

export function createMockMediaItem(overrides?: Partial<MediaItem>): MediaItem {
  return {
    id: 'test-id',
    filename: 'test.png',
    path: '2025/test.png',
    mime_type: 'image/png',
    thumbnail_url: '/api/v1/media/test-id/thumbnail',
    width: 896,
    height: 1216,
    file_size: 245760,
    created_at: '2025-01-01T00:00:00Z',
    modified_at: '2025-01-01T00:00:00Z',
    ...overrides,
  };
}
```

**Setup file for tests:**
```typescript
// frontend/src/setup-tests.ts
import '@testing-library/jest-dom';
import { server } from './test-utils/msw-handlers';

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());
```

## Test Strategy

- Each component test is independent — no shared state
- MSW mocks all API calls — no network during tests
- `npm test` runs all tests; `npx vitest run` in CI
- Target: 90%+ coverage for components and hooks in Wave 4
