import { join } from 'node:path';
import { expect, test, type APIRequestContext } from '@playwright/test';
import type { Layout } from '../src/lib/editor/validate.ts';
import { exportText, menu, openLayout, sorted, status, tile, waitEvent } from './layout-helpers.ts';

test.describe.configure({ mode: 'serial' });

async function companion(request: APIRequestContext) {
  const minted = await request.post('/__companion/session', { headers: { 'X-TouchDeck-Dev': '1' } });
  expect(minted.status()).toBe(200);
  const { token } = (await minted.json()) as { token: string };
  return (method: 'GET' | 'POST' | 'PUT' | 'PATCH', path: string, data?: unknown) =>
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

type Api = Awaited<ReturnType<typeof companion>>;

async function stored(api: Api, device: string): Promise<Layout | null> {
  const record = await api('GET', `/test/devices/${device}/record`);
  expect(record.status()).toBe(200);
  return ((await record.json()) as { layouts: { landscape: Layout | null } }).layouts.landscape;
}

async function pushElsewhere(api: Api, device: string, body: unknown) {
  const pushed = await api('PUT', `/devices/${device}/layouts/landscape`, body);
  expect(pushed.status()).toBe(202);
  const { id } = (await pushed.json()) as { id: string };
  await expect.poll(async () => ((await (await api('GET', `/jobs/${id}`)).json()) as { state: string }).state, { timeout: 30_000 }).toBe('succeeded');
}

test('the Layout screen sends a Layout to a mock Device, and the Device shows it', async ({ page, request }) => {
  const api = await companion(request);
  await openLayout(page);
  await expect(page.getByTestId('device-name')).toHaveText('Mock Device mock0001');
  await page.getByTestId('start-starter').click();
  await page.getByTestId('push').click();
  const sent = await waitEvent(page, 'pushed', 30_000);
  await expect(sent).toHaveText('Sent to Mock Device mock0001');
  await expect(sent).toHaveAttribute('data-pages', '2');
  await expect(sent).toHaveAttribute('data-tiles', '16');
  await expect(page.getByTestId('save-state')).toHaveAttribute('data-state', 'saved');
  const mine = JSON.parse(await exportText(page)) as Layout;
  expect(sorted((await stored(api, 'mock0001'))!)).toEqual(sorted(mine));
  await page.getByRole('button', { name: 'Home', exact: true }).click();
  const card = page.getByRole('list', { name: 'Devices' }).getByRole('listitem').first();
  await expect(card.locator('.device-scene')).toHaveAttribute('data-page', 'media');
  if (process.env.TD_SHOTS) await page.screenshot({ path: join(process.env.TD_SHOTS, 'layout-companion-pushed.png') });
});

test('a send after the mock Device changed offers Keep mine, and Load from replaces after a confirm', async ({ page, request }) => {
  const api = await companion(request);
  await openLayout(page);
  await page.getByTestId('start-pull').click();
  await expect(await waitEvent(page, 'pulled', 30_000)).toHaveText('Loaded “Starter” from Mock Device mock0001');
  await tile(page, 'prev').click();
  await page.getByTestId('tile-clear').click();

  await pushElsewhere(api, 'mock0001', layout);
  await page.getByTestId('push').click();
  const dialog = page.getByRole('alertdialog');
  await expect(dialog).toContainText('Mock Device mock0001 has “E2E”', { timeout: 30_000 });
  await expect(dialog).toContainText('(2 Pages, 1 Tile). Replace the landscape Layout here with it?');
  await dialog.getByRole('button', { name: 'Keep mine' }).click();
  await expect(await waitEvent(page, 'pushed', 30_000)).toHaveAttribute('data-tiles', '15');
  expect((await stored(api, 'mock0001'))?.name).toBe('Starter');

  await pushElsewhere(api, 'mock0001', layout);
  await menu(page, 'pull');
  await dialog.getByRole('button', { name: 'Replace' }).click();
  await expect(await waitEvent(page, 'pulled', 30_000)).toHaveText('Loaded “E2E” from Mock Device mock0001');
  expect(sorted(JSON.parse(await exportText(page)) as Layout)).toEqual(sorted((await stored(api, 'mock0001'))!));
  await expect(status(page)).toHaveAttribute('data-kind', 'ok');
});

interface MockRecord {
  settings: { orientation: string };
  reboots: number;
}

test('the Device panel renames a mock Device, switches its orientation with a restart, and restarts it', async ({ page, request }) => {
  const api = await companion(request);
  const record = async (): Promise<MockRecord> => (await api('GET', '/test/devices/mock0003/record')).json();
  const before = await record();
  await page.goto('/');
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.getByRole('button', { name: 'Edit Mock Device mock0003' }).click();
  await expect(page.getByTestId('editor')).toHaveAttribute('data-device-id', 'mock0003');
  await page.getByTestId('device-name').click();
  const status = page.getByTestId('device-panel-status');
  const name = page.getByTestId('device-panel-name');
  await expect(name).toBeEditable();

  await name.fill('Bench');
  await name.press('Enter');
  await expect(status).toHaveAttribute('data-event', 'saved');
  await expect(page.getByTestId('device-name')).toHaveText('Bench');
  const renamed: { name: string } = await (await api('GET', '/devices/mock0003')).json();
  expect(renamed.name).toBe('Bench');
  await expect(page.getByTestId('device-panel-brightness-hint')).toHaveText("Bench's Firmware can't change this yet.");

  await page.getByTestId('device-panel-portrait').click();
  await page.getByTestId('device-panel-confirm-restart').click();
  await expect(status).toHaveAttribute('data-event', 'restarting');
  await expect(status).not.toHaveAttribute('data-event', 'restarting', { timeout: 30_000 });
  await expect(status).not.toHaveAttribute('data-kind', 'error');
  await expect(page.getByTestId('device-panel-portrait')).toHaveAttribute('aria-pressed', 'true');
  const switched = await record();
  expect(switched.settings.orientation).toBe('portrait');
  expect(switched.reboots).toBe(before.reboots + 1);

  await page.getByTestId('device-panel-restart').click();
  await expect(status).toHaveAttribute('data-event', 'restarting');
  await expect(status).not.toHaveAttribute('data-event', 'restarting', { timeout: 30_000 });
  await expect(status).not.toHaveAttribute('data-kind', 'error');
  expect((await record()).reboots).toBe(before.reboots + 2);

  const restored = await api('PATCH', '/devices/mock0003/settings', { idle_timeout_s: null, orientation: 'landscape', name: 'Mock Device mock0003', brightness: null });
  expect(restored.status()).toBe(200);
});
