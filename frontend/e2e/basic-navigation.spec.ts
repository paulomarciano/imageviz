import { test, expect, type Page } from '@playwright/test';

/**
 * Helper: configure a watched folder pointing to test fixtures and trigger indexing.
 * Sends requests directly to the API for fast setup.
 */
async function setupTestData(page: Page) {
  const response = await page.request.put('http://localhost:3001/api/v1/config', {
    data: {
      watched_folders: [
        {
          path: '../test-fixtures',
          label: 'Test Fixtures',
        },
      ],
    },
  });
  expect(response.ok()).toBeTruthy();
}

test.describe('Basic Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await setupTestData(page);
    await page.goto('/');
  });

  test('grid displays thumbnails after indexing', async ({ page }) => {
    // Wait for grid to have thumbnail cards
    const thumbnails = page.locator('[role="button"][aria-label^="View"]');
    await expect(thumbnails.first()).toBeVisible({ timeout: 15000 });

    // Verify at least one thumbnail is rendered
    const count = await thumbnails.count();
    expect(count).toBeGreaterThan(0);
  });

  test('clicking thumbnail opens detail view', async ({ page }) => {
    // Wait for thumbnails
    const firstCard = page.locator('[role="button"][aria-label^="View"]').first();
    await expect(firstCard).toBeVisible({ timeout: 15000 });
    await firstCard.click();

    // Detail view modal should open
    const closeButton = page.locator('button[aria-label="Close detail view"]');
    await expect(closeButton).toBeVisible({ timeout: 5000 });

    // Close with Escape
    await page.keyboard.press('Escape');
    await expect(closeButton).not.toBeVisible({ timeout: 5000 });
  });

  test('config panel opens and shows settings', async ({ page }) => {
    // Click the settings button
    const settingsButton = page.locator('button[aria-label="Open settings"]');
    await expect(settingsButton).toBeVisible({ timeout: 10000 });
    await settingsButton.click();

    // Settings panel should be visible
    await expect(page.getByText('Watched Folders')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('Index Statistics')).toBeVisible({ timeout: 5000 });

    // Close with Escape
    await page.keyboard.press('Escape');
    await expect(page.getByText('Watched Folders')).not.toBeVisible({ timeout: 3000 });
  });

  test('keyboard shortcuts panel opens on ?', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(1000);

    // Press ? to open shortcuts
    await page.keyboard.press('?');

    // Shortcuts panel should be visible
    await expect(page.getByText('Keyboard Shortcuts')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('Global')).toBeVisible();
    await expect(page.getByText('Grid')).toBeVisible();

    // Close with Escape
    await page.keyboard.press('Escape');
    await expect(page.getByText('Keyboard Shortcuts')).not.toBeVisible({ timeout: 3000 });
  });
});
