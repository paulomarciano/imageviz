import path from 'path';
import { fileURLToPath } from 'url';
import { test, expect, type Page } from '@playwright/test';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
/** Absolute path to the project root's test-fixtures directory. */
const FIXTURES_DIR = path.resolve(__dirname, '../../test-fixtures');

/** Setup matching the other specs: configure fixtures as a watched folder via the API.
 * (Same pattern as basic-navigation/search-and-view — extracting a shared helper is
 * tracked in a follow-up ticket; see PR #112 review.) */
async function setupTestData(page: Page) {
  const response = await page.request.put('http://localhost:3001/api/v1/config', {
    data: {
      watched_folders: [
        {
          path: FIXTURES_DIR,
          label: 'Test Fixtures',
        },
      ],
    },
  });
  expect(response.ok()).toBeTruthy();
}

/**
 * Regression test for ticket #111: the detail-view next arrow used a hardcoded
 * `right-[340px]` to dodge the 320px metadata panel — but the panel is a flex
 * sibling of the viewer area, so the offset pushed the button ~340px INTO the
 * image area instead of stopping at its right edge. Blatant on portrait/vertical
 * screens, where the narrow viewer area puts the arrow near the image's center.
 *
 * The dialog's two direct children are [viewer area (relative), metadata panel];
 * the panel's left edge therefore IS the viewer area's right edge, which is what
 * both arrows must flank symmetrically (left-4 / right-4 = 16px inset).
 */
test.describe('Detail View Arrow Placement', () => {
  test.beforeEach(async ({ page }) => {
    await setupTestData(page);
  });

  test('prev/next arrows flank the viewer area symmetrically on a portrait viewport', async ({
    page,
  }) => {
    // Portrait/vertical screen: narrow viewport, tall aspect.
    await page.setViewportSize({ width: 720, height: 1280 });
    await page.goto('/');

    const firstCard = page.locator('[role="button"][aria-label^="View"]').first();
    await expect(firstCard).toBeVisible({ timeout: 15000 });
    await firstCard.click();

    await expect(
      page.locator('button[aria-label="Close detail view"]'),
    ).toBeVisible({ timeout: 5000 });

    // Index 0 hides the prev arrow; step right so both arrows render (≥2 fixtures).
    await expect(page.getByRole('button', { name: 'Next item' })).toBeVisible({
      timeout: 5000,
    });
    await page.keyboard.press('ArrowRight');

    const prevButton = page.getByRole('button', { name: 'Previous item' });
    const nextButton = page.getByRole('button', { name: 'Next item' });
    await expect(prevButton).toBeVisible({ timeout: 5000 });

    const dialogBox = await page.getByRole('dialog').boundingBox();
    // Last direct child of the dialog = metadata panel; its left edge == viewer area's right edge.
    const panelBox = await page.locator('[role="dialog"] > div').last().boundingBox();
    const prevBox = await prevButton.boundingBox();
    const nextBox = await nextButton.boundingBox();

    expect(dialogBox).not.toBeNull();
    expect(panelBox).not.toBeNull();
    expect(prevBox).not.toBeNull();
    expect(nextBox).not.toBeNull();
    if (!dialogBox || !panelBox || !prevBox || !nextBox) {
      throw new Error('boundingBox() returned null — geometry assertions would be vacuous');
    }

    const viewerLeft = dialogBox.x;
    const viewerRight = panelBox.x;
    const prevInset = prevBox.x - viewerLeft;
    const nextInset = viewerRight - (nextBox.x + nextBox.width);

    // left-4 / right-4 → both arrows inset 16px from the viewer area's edges.
    expect(Math.abs(prevInset - 16)).toBeLessThan(2);
    expect(Math.abs(nextInset - 16)).toBeLessThan(2);
  });
});
