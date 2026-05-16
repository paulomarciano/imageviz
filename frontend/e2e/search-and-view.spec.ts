import { test, expect, type Page } from '@playwright/test';

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

test.describe('Search and View', () => {
  test.beforeEach(async ({ page }) => {
    await setupTestData(page);
    await page.goto('/');
  });

  test('search input is accessible and functional', async ({ page }) => {
    // Search input should be visible
    const searchInput = page.locator('input[type="search"]');
    await expect(searchInput).toBeVisible({ timeout: 10000 });

    // Type a search query
    await searchInput.fill('test');
    await page.waitForTimeout(500); // Debounce

    // Clear button should appear
    const clearButton = page.locator('button[aria-label="Clear search"]');
    await expect(clearButton).toBeVisible({ timeout: 3000 });

    // Clear the search
    await clearButton.click();
    await expect(searchInput).toHaveValue('');
  });

  test('detail view close button works', async ({ page }) => {
    // Click first thumbnail
    const firstCard = page.locator('[role="button"][aria-label^="View"]').first();
    await expect(firstCard).toBeVisible({ timeout: 15000 });
    await firstCard.click();

    // Detail view close button should be visible
    const closeButton = page.locator('button[aria-label="Close detail view"]');
    await expect(closeButton).toBeVisible({ timeout: 5000 });

    // Click close button
    await closeButton.click();
    await expect(closeButton).not.toBeVisible({ timeout: 5000 });
  });

  test('settings can add and display a folder', async ({ page }) => {
    // Open settings
    const settingsButton = page.locator('button[aria-label="Open settings"]');
    await expect(settingsButton).toBeVisible({ timeout: 10000 });
    await settingsButton.click();

    // Add a folder
    const pathInput = page.locator('input[aria-label="Folder path"]');
    await expect(pathInput).toBeVisible({ timeout: 5000 });
    await pathInput.fill('/tmp');

    // Click Add
    await page.locator('button:has-text("Add")').click();

    // The folder should appear in the list
    await expect(page.getByText('/tmp')).toBeVisible({ timeout: 3000 });
  });
});
