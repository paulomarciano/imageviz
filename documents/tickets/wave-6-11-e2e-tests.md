# Wave 6.11 — Write E2E Tests (Playwright)

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 11 |
| **Estimate** | 3 hours |
| **Depends on** | 6.1–6.10 (all Wave 6 features) |
| **Parallel** | No (verifies entire frontend end-to-end) |

---

## Overview

Write end-to-end tests using Playwright that exercise full user journeys: configure watched folders → browse grid → search → view detail → close. These tests run against a real backend with real file system interactions.

## Prerequisites

- All frontend and backend features complete (Waves 0–6)
- Playwright installed (`npx playwright install`)
- `playwright` in devDependencies (from 0.3)
- Test fixtures in `test-fixtures/`

## Reference Files

- `documents/plans/development-plan.md` — §7.3 Frontend Testing (E2E with Playwright), §7.1 Testing Pyramid (~10 E2E tests)
- `.opencode/context/core/standards/test-coverage.md`

## Deliverables

```
frontend/e2e/
├── basic-navigation.spec.ts     # Grid browsing, scroll, click
└── search-and-view.spec.ts      # Search → filter → view → close journey
```

## Acceptance Criteria (Pass/Fail)

**basic-navigation.spec.ts:**
- [ ] Test: `grid_displays_thumbnails` — app loads, grid shows media items
- [ ] Test: `infinite_scroll_loads_more` — scrolling to bottom loads next page
- [ ] Test: `click_opens_detail_view` — clicking thumbnail opens detail modal
- [ ] Test: `escape_closes_detail_view` — pressing Escape closes modal
- [ ] Test: `arrow_navigation_in_detail` — ← → navigates between items

**search-and-view.spec.ts:**
- [ ] Test: `search_filters_grid` — typing in search shows filtered results
- [ ] Test: `search_clear_restores_full_list` — clearing search returns to full grid
- [ ] Test: `detail_view_metadata_visible` — detail view shows metadata panel
- [ ] Test: `video_viewer_plays` — clicking a video opens player with controls
- [ ] Test: `config_panel_save_folders` — open settings, add folder, save

## Implementation Notes

**Playwright configuration:**
```typescript
// frontend/playwright.config.ts
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  retries: 1,
  workers: 1, // Serial for E2E (shared backend state)
  
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  webServer: [
    {
      command: 'cd ../backend && cargo run',
      port: 3001,
      reuseExistingServer: true,
      timeout: 30000,
    },
    {
      command: 'npm run dev',
      port: 5173,
      reuseExistingServer: true,
      timeout: 15000,
    },
  ],
});
```

**Test — basic navigation:**
```typescript
// frontend/e2e/basic-navigation.spec.ts
import { test, expect } from '@playwright/test';

test.describe('Basic Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    
    // Configure a watched folder via API (or UI)
    // This ensures test data is available
  });

  test('grid displays thumbnails after indexing', async ({ page }) => {
    // Wait for grid to load
    const thumbnails = page.locator('[data-grid-index]');
    await expect(thumbnails.first()).toBeVisible({ timeout: 10000 });
    
    // Verify at least one thumbnail is rendered
    const count = await thumbnails.count();
    expect(count).toBeGreaterThan(0);
  });

  test('infinite scroll loads more items', async ({ page }) => {
    // Scroll to the bottom
    const grid = page.locator('main');
    await grid.evaluate((el) => el.scrollTo(0, el.scrollHeight));
    
    // Wait for new items to load
    await page.waitForTimeout(500);
    
    // Verify more items are visible
    const thumbnails = page.locator('[data-grid-index]');
    const count = await thumbnails.count();
    expect(count).toBeGreaterThan(10); // At least a few pages loaded
  });

  test('clicking thumbnail opens detail view', async ({ page }) => {
    // Click first thumbnail
    const firstCard = page.locator('[role="button"]').first();
    await firstCard.click();
    
    // Detail view modal should open
    const closeButton = page.locator('button[aria-label="Close detail view"]');
    await expect(closeButton).toBeVisible();
    
    // Close with Escape
    await page.keyboard.press('Escape');
    await expect(closeButton).not.toBeVisible();
  });

  test('arrow keys navigate between detail view items', async ({ page }) => {
    // Open first item
    await page.locator('[role="button"]').first().click();
    await expect(page.locator('button[aria-label="Close detail view"]')).toBeVisible();
    
    // Get current filename
    const firstFilename = await page.locator('.absolute.bottom-4 .font-medium').textContent();
    
    // Press right arrow
    await page.keyboard.press('ArrowRight');
    await page.waitForTimeout(300);
    
    // Filename should change
    const secondFilename = await page.locator('.absolute.bottom-4 .font-medium').textContent();
    expect(secondFilename).not.toBe(firstFilename);
  });
});
```

**Test — search and view:**
```typescript
// frontend/e2e/search-and-view.spec.ts
import { test, expect } from '@playwright/test';

test.describe('Search and View', () => {
  test('search filters grid and clear restores', async ({ page }) => {
    await page.goto('/');
    
    // Type in search bar
    const searchInput = page.locator('input[type="search"]');
    await searchInput.fill('sunset');
    
    // Wait for debounce and results
    await page.waitForTimeout(500);
    
    // Results should appear
    const resultsText = page.locator('text=/result.*"sunset"/i');
    await expect(resultsText).toBeVisible({ timeout: 5000 });
    
    // Clear search
    await page.locator('button[aria-label="Clear search"]').click();
    
    // Grid returns — verify "No media" or items present
    await page.waitForTimeout(300);
  });

  test('detail view shows metadata panel', async ({ page }) => {
    await page.goto('/');
    
    // Click a thumbnail
    await page.locator('[role="button"]').first().click();
    
    // Metadata panel should be visible
    const metadataHeading = page.locator('text=Metadata');
    await expect(metadataHeading).toBeVisible();
  });
});
```

**Test helpers for setup:**
```typescript
// Helper to seed test data via API
async function setupTestData(page: Page) {
  // Configure a watched folder pointing to test fixtures
  await page.request.put('http://localhost:3001/api/v1/config', {
    data: {
      watched_folders: [{
        path: '../test-fixtures',
        label: 'Test Fixtures',
      }],
    },
  });
  
  // Trigger indexing (if not automatic)
  await page.request.post('http://localhost:3001/api/v1/index');
  
  // Wait for indexing to complete
  await page.waitForTimeout(3000);
}
```

## Test Strategy

- E2E tests run against real backend and frontend dev servers
- Test data comes from `test-fixtures/` directory
- Tests are independent (each sets up its own state or uses shared state)
- Run with: `npx playwright test`
- Run with UI: `npx playwright test --ui`
- CI integration: add Playwright to CI workflow (0.9)

## External Docs

Use **ExternalScout** to fetch current Playwright docs for:
- Test configuration (`playwright.config.ts`)
- Locators and assertions
- API testing within E2E (`page.request`)
- Video recording and trace viewer
