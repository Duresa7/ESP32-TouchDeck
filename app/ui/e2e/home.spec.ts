import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

const demo = (page: Page) => ({
  push: (id: string, order: string[]) =>
    page.evaluate(([id, order]) => {
      if (!window.touchdeckDemo) throw new Error('The demo controls are missing.');
      window.touchdeckDemo.push(id, [...order]);
    }, [id, order] as const),
  offline: (id: string) =>
    page.evaluate((id) => {
      if (!window.touchdeckDemo) throw new Error('The demo controls are missing.');
      window.touchdeckDemo.offline(id);
    }, id),
});

for (const count of [0, 1, 2, 3, 4, 5]) {
  test(`${count} Devices keep their size and wrap with the design gap`, async ({ page }) => {
    await page.goto(`/?devices=${count}`);
    if (!count) {
      await expect(page.getByRole('heading', { name: 'Plug in your Device' })).toBeVisible();
      await expect(page.getByText('New board?', { exact: false })).toBeVisible();
      await page.getByRole('button', { name: 'Connection help' }).click();
      await expect(page.getByRole('heading', { name: 'Troubleshooting' })).toBeVisible();
      return;
    }
    const figures = page.locator('.device-figure');
    await expect(figures).toHaveCount(count);
    const boxes = await figures.evaluateAll((els) => els.map((el) => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width }; }));
    expect(boxes.every((b) => b.width === (count === 1 ? 560 : 312))).toBe(true);
    if (count > 1) expect(boxes[1].x - boxes[0].x).toBe(368);
    if (count > 3) expect(boxes[3].y).toBeGreaterThan(boxes[0].y);
  });
}

test('an unplugged Device keeps its Page and shows last seen', async ({ page }) => {
  await page.goto('/');
  const device = page.locator('.device-figure').nth(2);
  await expect(device.locator('.device-scene')).toHaveAttribute('data-page', 'media');
  await expect(device.locator('.device-spill')).toHaveCount(1);
  await demo(page).offline('mock0003');
  await expect(device).toContainText('Not connected');
  await expect(device).toContainText('Last seen');
  await expect(device.locator('.device-scene')).toHaveAttribute('data-page', 'media');
  await expect(device.locator('.device-scene')).toContainText('Play/Pause');
  await expect(device.locator('.device-spill')).toHaveCount(0);
  await expect(device.locator('.device-scene')).toHaveCSS('filter', 'grayscale(1)');
  await expect(page.getByText('Firmware update', { exact: true })).toHaveCount(1);
});

test('a pushed Layout reaches the miniature without reloading', async ({ page }) => {
  await page.goto('/');
  const scenes = page.locator('.device-scene');
  await expect(scenes.first()).toHaveAttribute('data-page', 'media');
  await page.evaluate(() => { document.documentElement.dataset.notReloaded = 'yes'; });
  await demo(page).push('mock0001', ['work', 'media']);
  await expect(scenes.first()).toHaveAttribute('data-page', 'work');
  await expect(scenes.first()).toContainText('Copy');
  await expect(scenes.nth(1)).toHaveAttribute('data-page', 'media');
  expect(await page.evaluate(() => document.documentElement.dataset.notReloaded)).toBe('yes');
});

for (const method of ['click', 'Enter', 'Space']) {
  test(`${method} opens the selected Device Layout`, async ({ page }) => {
    await page.goto('/');
    const device = page.getByRole('button', { name: 'Edit Studio' });
    if (method === 'click') await device.click();
    else { await device.focus(); await page.keyboard.press(method); }
    await expect(page.locator('main')).toHaveAttribute('data-device-id', 'mock0002');
    await expect(page.getByText('Choose what each Tile on Studio does.')).toBeVisible();
  });
}

test('keyboard focus is visible; fine-pointer lift stops with reduced motion', async ({ page }) => {
  await page.goto('/');
  const device = page.getByRole('button', { name: 'Edit Desk' });
  await expect(device).toBeVisible();
  await page.keyboard.press('Tab');
  await expect(device).toBeFocused();
  await expect(device).toHaveCSS('outline-width', '2px');
  await expect(device).toHaveCSS('outline-style', 'solid');
  await device.hover();
  await expect(device.locator('.device-bezel')).toHaveCSS('transform', 'matrix(1, 0, 0, 1, 0, -2)');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(device.locator('.device-bezel')).toHaveCSS('transform', 'none');
  await expect(device.locator('.device-bezel')).toHaveCSS('transition-duration', '0s');
});

for (const theme of ['light', 'dark'] as const) {
  for (const count of [0, 1, 3]) {
    test(`proof ${count} Devices ${theme}`, async ({ page }) => {
      test.skip(!process.env.TD_SHOTS, 'set TD_SHOTS for visual evidence');
      await page.emulateMedia({ colorScheme: theme });
      await page.goto(`/?devices=${count}`);
      await expect(page.locator(count ? '.device-scene[data-page]' : '.device-ghost')).toHaveCount(count || 1);
      if (count === 3) {
        await demo(page).offline('mock0003');
        await expect(page.getByText('Not connected', { exact: true })).toBeVisible();
      }
      await page.evaluate(() => document.fonts.ready);
      await page.screenshot({ path: join(process.env.TD_SHOTS!, `home-${count}-${theme}.png`) });
      if (count === 1) {
        await demo(page).offline('mock0001');
        await expect(page.getByText('Not connected', { exact: true })).toBeVisible();
        await page.screenshot({ path: join(process.env.TD_SHOTS!, `home-offline-${theme}.png`) });
      }
    });
  }
}
