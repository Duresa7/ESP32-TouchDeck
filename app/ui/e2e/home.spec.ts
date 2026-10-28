import { join } from 'node:path';
import { expect, test } from '@playwright/test';

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

test('offline keeps its Page and last seen; a live Page event updates without reloading', async ({ page }) => {
  await page.goto('/');
  const offline = page.locator('.device-figure').nth(2);
  await expect(offline).toContainText('Last seen');
  await expect(offline.locator('.device-scene')).toHaveAttribute('data-page', 'media');
  await expect(offline.locator('.device-spill')).toHaveCount(0);
  await expect(offline.locator('.device-scene')).toHaveCSS('filter', 'grayscale(1)');
  await expect(page.getByText('Firmware update', { exact: true })).toHaveCount(1);
  await page.evaluate(() => (window as unknown as { touchdeckDemo: { page(id: string, page: string): void } }).touchdeckDemo.page('mock0001', 'work'));
  await expect(page.locator('.device-scene').first()).toHaveAttribute('data-page', 'work');
  await expect(page.locator('.device-scene').first()).toContainText('Copy');
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
  await page.keyboard.press('Tab');
  const device = page.getByRole('button', { name: 'Edit Desk' });
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
      await expect(page.locator(count ? '.device-figure' : '.device-ghost').first()).toBeVisible();
      await page.evaluate(() => document.fonts.ready);
      await page.screenshot({ path: join(process.env.TD_SHOTS!, `home-${count}-${theme}.png`) });
      if (count === 1) {
        await page.evaluate(() => (window as unknown as { touchdeckDemo: { offline(id: string): void } }).touchdeckDemo.offline('mock0001'));
        await expect(page.getByText('Not connected', { exact: true })).toBeVisible();
        await page.screenshot({ path: join(process.env.TD_SHOTS!, `home-offline-${theme}.png`) });
      }
    });
  }
}
