import { join } from 'node:path';
import { expect, test } from '@playwright/test';

// Review screenshots at 1180 x 760, light and dark. They're written only when
// TD_SHOTS names a folder: TD_SHOTS=<folder> pnpm e2e screens
const folder = process.env.TD_SHOTS;

for (const theme of ['light', 'dark'] as const) {
  test.describe(theme, () => {
    test.skip(!folder, 'set TD_SHOTS to a folder to take screenshots');
    test.use({ colorScheme: theme });

    test.beforeEach(async ({ page }) => {
      await page.goto('/');
      await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
      await page.evaluate(() => document.fonts.ready);
    });

    test('home with the demo Devices', async ({ page }) => {
      await page.screenshot({ path: join(folder!, `home-${theme}.png`) });
    });

    test('settings opened from the keyboard, focus on its first control', async ({ page }) => {
      await page.getByRole('button', { name: 'Settings' }).focus();
      await page.keyboard.press('Enter');
      const dialog = page.getByRole('dialog', { name: 'Settings' });
      await expect(dialog).toBeVisible();
      await expect(dialog.locator(':focus-visible')).toHaveCount(1);
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(folder!, `settings-focus-${theme}.png`) });
    });

    test('keyboard focus on the nav cluster', async ({ page }) => {
      await page.getByRole('button', { name: 'Home', exact: true }).focus();
      await page.keyboard.press('Tab');
      await expect(page.locator('.nav-button:focus-visible')).toHaveAttribute('aria-label', 'Layout');
      await page.screenshot({ path: join(folder!, `focus-nav-${theme}.png`) });
    });
  });
}
