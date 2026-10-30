import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

const folder = process.env.TD_SHOTS;
const dialog = (page: Page) => page.getByTestId('onboarding');

async function home(page: Page, devices = 1) {
  await page.goto(`/?devices=${devices}`);
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
}

async function shoot(page: Page, name: string, theme: string) {
  await page.mouse.move(0, 0);
  await page.evaluate(() => Promise.all(document.getAnimations().map((animation) => animation.finished)));
  await page.screenshot({ path: join(folder!, `${name}-${theme}.png`) });
}

for (const theme of ['light', 'dark'] as const) {
  test.describe(theme, () => {
    test.skip(!folder, 'set TD_SHOTS to a folder to take screenshots');
    test.use({ colorScheme: theme });

    test('each onboarding step', async ({ page }) => {
      await home(page);
      await page.evaluate(() => window.touchdeckDemo?.fresh('mock0009', { version: '1.4.0', protocol: 2 }));
      await expect(dialog(page)).toHaveAttribute('data-step', 'name');
      await page.getByTestId('onboarding-name').fill('Shelf');
      await shoot(page, 'onboarding-1-name', theme);
      await page.getByTestId('onboarding-next').click();
      await expect(dialog(page)).toHaveAttribute('data-step', 'orientation');
      await shoot(page, 'onboarding-2-orientation', theme);
      await page.getByTestId('onboarding-next').click();
      await expect(dialog(page)).toHaveAttribute('data-step', 'layout');
      await expect(page.getByTestId('onboarding-next')).toHaveText('Next');
      await shoot(page, 'onboarding-3-layout', theme);
      await page.getByTestId('onboarding-next').click();
      await expect(dialog(page)).toHaveAttribute('data-step', 'firmware');
      await shoot(page, 'onboarding-4-firmware', theme);
      await page.evaluate(() => window.touchdeckDemo?.firmware.hold('mock0009'));
      await page.getByTestId('onboarding-update').click();
      await expect(page.getByTestId('onboarding-firmware-status')).toHaveText('Updating… 40%');
      await shoot(page, 'onboarding-4-firmware-updating', theme);
      await page.evaluate(() => window.touchdeckDemo?.firmware.release('mock0009'));
      await expect(page.getByTestId('onboarding-firmware-status')).toHaveText('Updated to 1.5.0', { timeout: 10_000 });
      await shoot(page, 'onboarding-4-firmware-updated', theme);
      await page.getByTestId('onboarding-next').click();
      await expect(dialog(page)).toHaveAttribute('data-step', 'done');
      await shoot(page, 'onboarding-5-done', theme);
    });

    test('the New board card while installing', async ({ page }) => {
      await home(page, 1);
      await page.evaluate(() => window.touchdeckDemo?.newBoards.plug({ id: 'mock00ff', port: 'COM7' }));
      const card = page.getByTestId('new-board');
      await expect(card).toHaveAttribute('data-step', 'detected');
      await shoot(page, 'new-board-found', theme);
      await page.evaluate(() => window.touchdeckDemo?.newBoards.hold('mock00ff', 60));
      await card.getByTestId('board-install').click();
      await expect(card).toHaveAttribute('data-step', 'waiting');
      await shoot(page, 'new-board-waiting', theme);
      await page.evaluate(() => window.touchdeckDemo?.newBoards.answer('mock00ff', 'accept'));
      await expect(card.getByTestId('board-status')).toHaveText('Installing Firmware… 60%');
      await shoot(page, 'new-board-installing', theme);
    });
  });
}
