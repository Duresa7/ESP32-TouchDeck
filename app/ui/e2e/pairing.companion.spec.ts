import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { expect, test } from '@playwright/test';

test.describe.configure({ mode: 'serial' });
test.use({ trace: 'off', screenshot: 'off', video: 'off' });
let process: ChildProcess;
let data: string;
let origin: string;
let token: string;

test.beforeAll(async ({ request }) => {
  data = await mkdtemp(join(tmpdir(), 'touchdeck-pairing-'));
  process = spawn(resolve('../target/debug/td-companion.exe'), ['--mock-devices', '2'], {
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

for (const delivery of ['link', 'manual']) {
  test(`pairs through native ${delivery} delivery`, async ({ page, request }) => {
    const headers = { Authorization: `Bearer ${token}` };
    const minted = await request.post(`${origin}/api/v1/auth/pair-codes`, { headers });
    expect(minted.status()).toBe(200);
    expect(Object.keys(await minted.json())).toEqual(['expires_ms']);
    const capture: unknown = await (await request.get(`${origin}/api/v1/test/pairing-delivery`, { headers })).json();
    if (typeof capture !== 'object' || capture === null || !('url' in capture) || typeof capture.url !== 'string' || !('notification' in capture) || typeof capture.notification !== 'string') throw new Error('Native delivery missing');
    if (delivery === 'link') await page.goto(capture.url);
    else {
      await page.goto(`${origin}/pair`);
      await page.getByLabel('Code', { exact: true }).fill(capture.notification);
      await page.getByRole('button', { name: 'Pair', exact: true }).click();
    }
    await expect(page).toHaveURL(`${origin}/`);
    await expect(page.getByRole('list', { name: 'Devices' }).getByRole('listitem')).toHaveCount(2);
    expect(await page.evaluate(() => !!localStorage.getItem('touchdeck.session'))).toBe(true);
    await page.reload();
    await expect(page.getByRole('list', { name: 'Devices' }).getByRole('listitem')).toHaveCount(2);
  });
}

test('removes fragment and query before rejecting a wrong code', async ({ page }) => {
  await page.goto(`${origin}/pair?code=wrong#code=also-wrong`);
  await expect(page).toHaveURL(`${origin}/pair`);
  await expect(page.getByRole('alert')).toContainText('expired or is wrong');
});
