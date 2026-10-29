import { join } from 'node:path';
import { expect, test, type APIRequestContext } from '@playwright/test';

// Runs with playwright.companion.config.ts, against `td-companion --mock-devices 3`.
// One Companion serves every test, and a later test unplugs a Device, so they run in order.
test.describe.configure({ mode: 'serial' });

/** A window session from the dev server, as the page gets one, for driving the Companion. */
async function companion(request: APIRequestContext) {
  const minted = await request.post('/__companion/session', { headers: { 'X-TouchDeck-Dev': '1' } });
  expect(minted.status()).toBe(200);
  const { token } = (await minted.json()) as { token: string };
  return (method: 'GET' | 'POST' | 'PUT', path: string, data?: unknown) =>
    request.fetch(`/api/v1${path}`, { method, data, headers: { Authorization: `Bearer ${token}` } });
}

const layout = {
  schema: 1, name: 'E2E', orientation: 'landscape',
  pages: [
    { id: 'keys', title: 'Keys', kind: 'grid', rows: 2, cols: 2, tiles: [
      { id: 'copy', label: 'Copy', row: 0, col: 0, rowSpan: 1, colSpan: 1, icon: null, action: { type: 'none' } },
    ] },
    { id: 'more', title: 'More', kind: 'grid', rows: 1, cols: 1, tiles: [] },
  ],
};

test('Home lists the mock Devices from the real Companion and opens one', async ({ page }) => {
  const session = page.waitForResponse((r) => r.url().endsWith('/__companion/session'));
  const events = page.waitForEvent('websocket', (ws) => new URL(ws.url()).pathname === '/api/v1/events');
  await page.goto('/');

  expect((await session).status()).toBe(200);
  await events;

  const cards = page.getByRole('list', { name: 'Devices' }).getByRole('listitem');
  await expect(cards).toHaveCount(3);
  for (let i = 0; i < 3; i++) {
    await expect(cards.nth(i)).toContainText(`Mock Device mock000${i + 1}`);
    await expect(cards.nth(i)).toContainText('Connected');
    await expect(cards.nth(i).locator('.device-spill')).toHaveCount(1);
  }
  const widths = await cards.evaluateAll((els) => els.map((el) => el.getBoundingClientRect().width));
  expect(widths).toEqual([312, 312, 312]);

  await cards.nth(2).getByRole('button').press('Enter');
  await expect(page.locator('main')).toHaveAttribute('data-device-id', 'mock0003');
});

test('a Layout pushed to a Device shows on its miniature, and stays when it is unplugged', async ({ page, request }) => {
  const api = await companion(request);
  await page.goto('/');
  const card = page.getByRole('list', { name: 'Devices' }).getByRole('listitem').nth(1);
  await expect(card).toContainText('Connected');
  // A mock Device starts with no stored Layout, so its screen is blank.
  await expect(card.locator('.device-scene')).not.toHaveAttribute('data-page');
  await page.evaluate(() => { (window as unknown as { notReloaded: boolean }).notReloaded = true; });

  const pushed = await api('PUT', '/devices/mock0002/layouts/landscape', layout);
  expect(pushed.status()).toBe(202);
  await expect(card.locator('.device-scene')).toHaveAttribute('data-page', 'keys');
  await expect(card.locator('.device-scene')).toContainText('Keys');
  await expect(card.locator('.device-scene')).toContainText('Copy');
  expect(await page.evaluate(() => (window as unknown as { notReloaded?: boolean }).notReloaded)).toBe(true);

  expect((await api('POST', '/test/ports', { unplug: 'mock0002' })).status()).toBe(204);
  await expect(card).toContainText('Not connected');
  await expect(card).toContainText('Last seen');
  await expect(card.locator('.device-spill')).toHaveCount(0);
  await expect(card.locator('.device-scene')).toHaveAttribute('data-page', 'keys');
  await expect(card.locator('.device-scene')).toContainText('Copy');
  if (process.env.TD_SHOTS) await page.screenshot({ path: join(process.env.TD_SHOTS, 'home-companion-3.png') });
});
