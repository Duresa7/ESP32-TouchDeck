import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { openCopyTile, openPicker, option, picker } from './icon-picker-helpers.ts';

const folder = process.env.TD_SHOTS ?? '';

async function shoot(page: Page, name: string) {
  await page.mouse.move(0, 0);
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(folder, name), animations: 'disabled' });
}

for (const theme of ['light', 'dark'] as const) {
  test.describe(theme, () => {
    test.skip(!folder, 'set TD_SHOTS to a folder to take screenshots');
    test.use({ colorScheme: theme });

    test('recent picks and built-in icons', async ({ page }) => {
      await openCopyTile(page);
      await openPicker(page);
      await page.getByTestId('icon-search').fill('spotify');
      await option(page, 'hit:simple-icons:spotify').click();
      await expect(picker(page)).toBeHidden();
      await openPicker(page);
      await option(page, 'lv:play_pause').click();
      await expect(picker(page)).toBeHidden();
      await openPicker(page);
      await expect(picker(page).getByTestId('icon-section-recent')).toBeVisible();
      await shoot(page, `icon-picker-recent-${theme}.png`);
    });

    test('logo results', async ({ page }) => {
      await openCopyTile(page);
      await openPicker(page);
      await page.getByTestId('icon-search').fill('home');
      await expect(picker(page).getByTestId('icon-section-logos')).toBeVisible();
      await shoot(page, `icon-picker-results-${theme}.png`);
    });

    test('a render failure', async ({ page }) => {
      await openCopyTile(page);
      await page.evaluate(() => window.touchdeckDemo?.assets.failRender('plex'));
      await openPicker(page);
      await page.getByTestId('icon-search').fill('plex');
      await option(page, 'hit:simple-icons:plex').click();
      await expect(picker(page).getByTestId('icon-error')).toBeVisible();
      await shoot(page, `icon-picker-error-${theme}.png`);
    });

    test('the Companion offline', async ({ page }) => {
      await openCopyTile(page);
      await openPicker(page);
      await page.evaluate(() => window.touchdeckDemo?.assets.offline(true));
      await page.getByTestId('icon-search').fill('plex');
      await expect(picker(page).getByTestId('icon-search-error')).toBeVisible();
      await shoot(page, `icon-picker-offline-${theme}.png`);
    });

    test('worst-case names', async ({ page }) => {
      await page.addInitScript(() => {
        const names = ['Calibre-Web-Automated-Book-Downloader.png', '日本語のロゴ.png', 'J', 'Wiśniewska-Kowalczyk team logo final v12.jpg'];
        localStorage.setItem('touchdeck.recent-icons', JSON.stringify(names.map((name, i) => ({ kind: 'symbol', icon: ['lv:prev', 'lv:next', 'lv:mute', 'lv:volume_max'][i], name }))));
      });
      await openCopyTile(page, '/?icons=worst');
      await openPicker(page);
      await shoot(page, `icon-picker-worst-recent-${theme}.png`);
      await page.getByTestId('icon-search').fill('logo');
      await expect(picker(page).getByTestId('icon-section-logos')).toBeVisible();
      await shoot(page, `icon-picker-worst-results-${theme}.png`);
    });
  });
}
