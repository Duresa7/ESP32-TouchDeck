import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

const folder = process.env.TD_SHOTS ?? '';

async function openSettings(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  await page.getByRole('button', { name: 'Settings' }).click();
  const dialog = page.getByRole('dialog', { name: 'Settings' });
  await expect(dialog.getByRole('switch', { name: 'Start at sign-in' })).toBeEnabled();
  await expect(dialog.getByTestId('paired-browser')).toHaveCount(2);
  return dialog;
}

for (const theme of ['light', 'dark'] as const) {
  test.describe(theme, () => {
    test.skip(!folder, 'set TD_SHOTS to a folder to take screenshots');
    test.use({ colorScheme: theme });

    test('settings', async ({ page }) => {
      await openSettings(page);
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(folder, `settings-${theme}.png`) });
    });

    test('settings, keyboard focus on Start at sign-in', async ({ page }) => {
      const dialog = await openSettings(page);
      await dialog.getByRole('switch', { name: 'Start at sign-in' }).focus();
      await page.keyboard.press('Space');
      await expect(dialog.getByRole('switch', { name: 'Start at sign-in' })).toBeChecked();
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(folder, `settings-switch-focus-${theme}.png`) });
    });

    test('reset pairing asks first', async ({ page }) => {
      const dialog = await openSettings(page);
      await dialog.getByRole('button', { name: 'Reset', exact: true }).click();
      await expect(page.getByRole('alertdialog', { name: 'Reset pairing?' })).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(folder, `settings-reset-${theme}.png`) });
    });

    test('troubleshooting', async ({ page }) => {
      const dialog = await openSettings(page);
      await dialog.getByRole('button', { name: /^Troubleshooting/ }).click();
      const help = page.getByRole('dialog', { name: 'Troubleshooting' });
      await expect(help.getByTestId('device-details')).toHaveCount(3);
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(folder, `troubleshooting-${theme}.png`) });
    });

    test('troubleshooting, the console after a reply and a refused line', async ({ page }) => {
      const dialog = await openSettings(page);
      await dialog.getByRole('button', { name: /^Troubleshooting/ }).click();
      const help = page.getByRole('dialog', { name: 'Troubleshooting' });
      const field = help.getByRole('textbox', { name: 'Console command' });
      await field.fill('version');
      await field.press('Enter');
      await expect(help.getByTestId('console-log').locator('[data-kind="line"]')).toHaveCount(1);
      await field.fill('reboot');
      await field.press('Enter');
      await expect(help.getByTestId('console-forbidden')).toBeVisible();
      await help.getByTestId('console-forbidden').scrollIntoViewIfNeeded();
      await page.screenshot({ path: join(folder, `troubleshooting-console-${theme}.png`) });
    });
  });
}
