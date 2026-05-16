# Wave 5.9 — Write Integration Tests (Search + Detail + Drag)

| Field | Value |
|-------|-------|
| **Wave** | 5 — Frontend: Search, Detail View & Drag-and-Drop |
| **Seq** | 09 |
| **Estimate** | 2.5 hours |
| **Depends on** | 5.1–5.8 (all Wave 5 features) |
| **Parallel** | No (verifies entire Wave 5) |

---

## Overview

Write integration tests that simulate real user journeys: searching for an item, clicking a thumbnail to open the detail view, navigating between items, viewing metadata, and closing the detail view. Tests use Vitest + Testing Library with MSW for API mocking.

## Prerequisites

- All Wave 5 components (5.1–5.8)
- MSW configured (from 4.10)
- Test render utility with providers (from 4.10)

## Reference Files

- `documents/plans/development-plan.md` — §7.3 Frontend Testing (integration tests), §7.5 Test Data Strategy
- `.opencode/context/core/standards/test-coverage.md`
- `frontend/src/__tests__/integration/` — existing patterns

## Deliverables

```
frontend/src/__tests__/integration/
├── search-flow.test.tsx         # Search → filter grid → view item journey
└── detail-view-flow.test.tsx    # Click → detail view → navigate → close journey
```

## Acceptance Criteria (Pass/Fail)

**Search flow tests:**
- [ ] Test: `search_filters_grid`: typing in search bar → grid shows search results → clearing search returns to full list
- [ ] Test: `search_shows_results_count`: search displays "X results for 'query'"
- [ ] Test: `search_no_results_message`: searching for nonexistent term shows empty state
- [ ] Test: `search_debounced`: rapid typing triggers only one API call (after 300ms)

**Detail view flow tests:**
- [ ] Test: `click_thumbnail_opens_detail_view`: clicking a card opens the modal with correct content
- [ ] Test: `escape_closes_detail_view`: pressing Escape closes the modal and returns to grid
- [ ] Test: `arrow_keys_navigate_between_items`: ← → in detail view loads adjacent items
- [ ] Test: `detail_view_shows_metadata`: metadata panel renders prompt/workflow data
- [ ] Test: `detail_view_shows_loading_state`: loading spinner while item detail is fetched
- [ ] Test: `close_button_closes_view`: clicking × button closes the detail view
- [ ] Test: `video_viewer_renders`: selecting a video item shows video player
- [ ] Test: `image_viewer_renders`: selecting an image shows image with zoom capability

## Implementation Notes

**Search flow test:**
```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../../test-utils/render-utils';
import App from '../../App'; // Full app or composed view

describe('Search Flow', () => {
  it('filters grid when searching and returns on clear', async () => {
    renderWithProviders(<App />);

    // Wait for initial grid to load
    await waitFor(() => {
      expect(screen.getByText('image_0.png')).toBeInTheDocument();
    });

    // Type in search bar
    const searchInput = screen.getByPlaceholderText('Search media...');
    await userEvent.type(searchInput, 'sunset');

    // Wait for debounce and search results
    await waitFor(() => {
      expect(screen.getByText(/result for "sunset"/)).toBeInTheDocument();
    }, { timeout: 2000 });

    // Clear search
    await userEvent.click(screen.getByLabelText('Clear search'));
    
    // Grid returns to full list
    await waitFor(() => {
      expect(screen.getByText('image_0.png')).toBeInTheDocument();
    });
  });
});
```

**Detail view flow test:**
```tsx
describe('Detail View Flow', () => {
  it('opens detail view on thumbnail click and closes on Escape', async () => {
    const { container } = renderWithProviders(<App />);

    // Wait for grid
    await waitFor(() => {
      expect(screen.getByText('image_0.png')).toBeInTheDocument();
    });

    // Click first thumbnail
    const firstCard = screen.getAllByRole('button')[0];
    await userEvent.click(firstCard);

    // Detail view should open
    await waitFor(() => {
      expect(screen.getByLabelText('Close detail view')).toBeInTheDocument();
    });

    // The image should be rendered in the viewer
    expect(screen.getByAltText('image_0.png')).toBeInTheDocument();

    // Press Escape
    await userEvent.keyboard('{Escape}');

    // Detail view should close
    await waitFor(() => {
      expect(screen.queryByLabelText('Close detail view')).not.toBeInTheDocument();
    });
  });

  it('navigates between items with arrow keys in detail view', async () => {
    renderWithProviders(<App />);

    await waitFor(() => screen.getByText('image_0.png'));
    await userEvent.click(screen.getAllByRole('button')[0]);

    await waitFor(() => screen.getByAltText('image_0.png'));

    // Navigate right
    await userEvent.keyboard('{ArrowRight}');
    await waitFor(() => {
      expect(screen.getByAltText('image_1.png')).toBeInTheDocument();
    });

    // Navigate left
    await userEvent.keyboard('{ArrowLeft}');
    await waitFor(() => {
      expect(screen.getByAltText('image_0.png')).toBeInTheDocument();
    });
  });
});
```

**MSW handlers for integration tests:**
```typescript
// Extend the MSW handlers to support detail endpoints
http.get('/api/v1/media/:id', ({ params }) => {
  const id = params.id as string;
  return HttpResponse.json({
    id,
    filename: `detail_${id}.png`,
    path: `2025/detail_${id}.png`,
    mime_type: 'image/png',
    thumbnail_url: `/api/v1/media/${id}/thumbnail`,
    file_url: `/api/v1/media/${id}/file`,
    width: 896,
    height: 1216,
    file_size: 245760,
    created_at: '2025-01-01T00:00:00Z',
    modified_at: '2025-01-01T00:00:00Z',
    metadata: {
      prompt: { seed: 12345, positive_prompt: 'a beautiful landscape' },
      workflow: { nodes: [{ id: 1, type: 'KSampler' }] },
    },
  });
}),
```

## Test Strategy

- All integration tests pass with `npx vitest run`
- Tests simulate real user journeys (not implementation details)
- MSW handles all API mocking — no real server needed
- Tests use `waitFor` and `findBy` queries for async operations
- Test files are co-located under `frontend/src/__tests__/integration/`
