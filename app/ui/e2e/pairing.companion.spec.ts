import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { expect, test, type APIRequestContext } from '@playwright/test';

test.describe.configure({ mode: 'serial' });
test.use({ trace: 'off', screenshot: 'off', video: 'off' });
let process: ChildProcess;
let data: string;
let origin: string;
let token: string;

test.beforeAll(async ({ request }) => {
  data = await mkdtemp(join(tmpdir(), 'touchdeck-pairing-'));
  process = spawn(resolve('../target/debug', globalThis.process.platform === 'win32' ? 'td-companion.exe' : 'td-companion'), ['--mock-devices', '1'], {
    env: { ...globalThis.process.env, TOUCHDECK_APP_DATA_DIR: data }, stdio: 'ignore', windowsHide: true,
  });
  const runtimePath = join(data, 'runtime', 'companion.json');
  await expect.poll(async () => {
    try {
      const runtime: unknown = JSON.parse(await readFile(runtimePath, 'utf8'));
      if (typeof runtime !== 'object' || runtime === null || !('port' in runtime) || typeof runtime.port !== 'number' || !('token' in runtime) || typeof runtime.token !== 'string') return false;
      origin = `http://127.0.0.1:${runtime.port}`;
      const response = await request.post(`${origin}/api/v1/auth/sessions`, { headers: { Authorization: `Bearer ${runtime.token}` }, data: { kind: 'window' } });
      const session: unknown = await response.json();
      if (typeof session !== 'object' || session === null || !('token' in session) || typeof session.token !== 'string') return false;
      token = session.token;
      return true;
    } catch { return false; }
  }).toBe(true);
});
test.afterAll(async ({ request }) => {
  if (origin && token) await request.post(`${origin}/api/v1/system/shutdown`, { headers: { Authorization: `Bearer ${token}` } }).catch(() => {});
  if (process && process.exitCode === null) await new Promise<void>(done => { process.once('exit', () => done()); process.kill(); });
  if (data) await rm(data, { recursive: true, force: true });
});

async function pairingLink(request: APIRequestContext) {
  const headers = { Authorization: `Bearer ${token}` };
  const minted = await request.post(`${origin}/api/v1/auth/pair-codes`, { headers });
  expect(minted.status()).toBe(200);
  expect(Object.keys(await minted.json())).toEqual(['expires_ms']);
  const capture: unknown = await (await request.get(`${origin}/api/v1/test/pairing-delivery`, { headers })).json();
  if (typeof capture !== 'object' || capture === null || !('url' in capture) || typeof capture.url !== 'string' || !('notification' in capture) || typeof capture.notification !== 'string') throw new Error('Native delivery missing');
  return { url: capture.url, notification: capture.notification };
}

for (const delivery of ['link', 'manual']) {
  test(`pairs through native ${delivery} delivery`, async ({ page, request }) => {
    const capture = await pairingLink(request);
    if (delivery === 'link') await page.goto(capture.url);
    else {
      await page.goto(`${origin}/pair`);
      await page.getByLabel('Code', { exact: true }).fill(capture.notification);
      await page.getByRole('button', { name: 'Pair', exact: true }).click();
    }
    await expect(page).toHaveURL(`${origin}/`);
    await expect(page.getByRole('list', { name: 'Devices' }).getByRole('listitem')).toHaveCount(1);
    expect(await page.evaluate(() => !!localStorage.getItem('touchdeck.session'))).toBe(true);
    await page.reload();
    await expect(page.getByRole('list', { name: 'Devices' }).getByRole('listitem')).toHaveCount(1);
  });
}

test('removes fragment and query before rejecting a wrong code', async ({ page }) => {
  await page.goto(`${origin}/pair?code=wrong#code=also-wrong`);
  await expect(page).toHaveURL(`${origin}/pair`);
  await expect(page.getByRole('alert')).toContainText('expired or is wrong');
});


test('Troubleshooting shows mock hello facts and Forget all signs browsers out in one request', async ({ page, browser, request }) => {
  const capture = await pairingLink(request);
  await page.goto(capture.url);
  await expect(page).toHaveURL(`${origin}/`);
  const other = await pairingLink(request);
  const paired = await browser.newContext();
  try {
    const second = await paired.newPage();
    await second.goto(other.url);
    await expect(second).toHaveURL(`${origin}/`);
    const windowContext = await browser.newContext();
    try {
      await windowContext.addInitScript((sessionToken) => localStorage.setItem('touchdeck.session', sessionToken), token);
      const window = await windowContext.newPage();
      await window.goto(origin);
      await window.getByRole('navigation', { name: 'Main' }).getByRole('button', { name: 'Settings' }).click();
      await window.getByRole('button', { name: /^Troubleshooting/ }).click();
      const details = window.getByTestId('device-details');
      for (const [key, value] of [['product', 'touchdeck'], ['running', 'ota_0'], ['sinks', 'log, link'], ['layout', 'builtin']]) {
        await expect(details.locator(`[data-fact="${key}"] dd`)).toHaveText(value);
      }
      await window.screenshot({ path: test.info().outputPath('troubleshooting.png') });
      await window.getByRole('button', { name: 'Back', exact: true }).click();
      const deletes: string[] = [];
      window.on('request', req => { if (req.method() === 'DELETE') deletes.push(req.url()); });
      await window.getByRole('button', { name: 'Forget all', exact: true }).click();
      await window.getByRole('alertdialog').getByRole('button', { name: 'Forget all', exact: true }).click();
      await expect(window.getByTestId('paired-browser')).toHaveCount(0);
      await expect(page).toHaveURL(`${origin}/pair`);
      await expect(second).toHaveURL(`${origin}/pair`);
      expect(deletes).toEqual([`${origin}/api/v1/auth/sessions?kind=browser`]);
      await expect(window.getByRole('dialog', { name: 'Settings', exact: true })).toBeVisible();
    } finally { await windowContext.close(); }
  } finally { await paired.close(); }
});
