import { test, expect, _electron as electron } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

// Build with `cargo build -p td-companion --features test-hooks` in app/, or point TOUCHDECK_COMPANION_BIN at a build.
const binary = process.env.TOUCHDECK_COMPANION_BIN ||
  resolve('../target/debug/td-companion' + (process.platform === 'win32' ? '.exe' : ''));

test('window loads from the real Companion and its session lists the mock Device', async () => {
  expect(existsSync(binary), `Companion binary missing at ${binary}`).toBe(true);
  const dir = await mkdtemp(join(tmpdir(), 'touchdeck-companion-'));
  const env = { ...process.env, TOUCHDECK_APP_DATA_DIR: dir, TOUCHDECK_COMPANION_BIN: binary };
  delete env.ELECTRON_RUN_AS_NODE;
  const companion = spawn(binary, ['--mock-devices', '1'], { env, windowsHide: true, stdio: 'ignore' });
  const runtimeFile = join(dir, 'runtime', 'companion.json');
  try {
    await expect.poll(() => existsSync(runtimeFile), { timeout: 10_000 }).toBe(true);
    const { port } = JSON.parse(await readFile(runtimeFile, 'utf8'));
    const origin = `http://127.0.0.1:${port}`;
    const app = await electron.launch({ args: [resolve('.')], env });
    try {
      const page = await app.firstWindow();
      await page.waitForURL(origin + '/');
      expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].getTitle())).toBe('Open TouchDeck');
      // The Companion serves no UI page at / until the React UI lands, so the call runs in whatever page loaded.
      const result = await page.evaluate(async () => {
        const token = await (window as any).openTouchDeck.session();
        const response = await fetch('/api/v1/devices', { headers: { Authorization: `Bearer ${token}` } });
        return { token, status: response.status, body: await response.json() };
      });
      expect(result.status).toBe(200);
      const devices = Array.isArray(result.body) ? result.body : result.body.devices;
      expect(devices.map((d: { id: string }) => d.id)).toEqual([expect.stringContaining('mock0001')]);
      const evidence = process.env.TOUCHDECK_EVIDENCE_DIR || resolve('../../.verify/72');
      await mkdir(evidence, { recursive: true });
      await page.screenshot({ path: resolve(evidence, 'electron-companion.png') });
      await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].close());
      // Closing revoked the window session on the Companion.
      await expect.poll(async () => (await fetch(`${origin}/api/v1/devices`,
        { headers: { Authorization: `Bearer ${result.token}` } })).status).toBe(401);
    } finally { await app.close(); }
  } finally {
    companion.kill();
    await rm(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
});
