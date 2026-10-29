import { test, expect, _electron as electron } from '@playwright/test';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fakeCompanion } from './fake-companion.mjs';

test('isolated shell, exact preload, navigation, renewal, single instance and close', async () => {
  const fake = await fakeCompanion();
  const env = { ...process.env, TOUCHDECK_APP_DATA_DIR: fake.dir };
  delete env.ELECTRON_RUN_AS_NODE;
  const app = await electron.launch({ args: [resolve('.')], env });
  try {
    const page = await app.firstWindow();
    await page.waitForURL(fake.origin + '/');
    expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].getTitle())).toBe('Open TouchDeck');
    expect(await page.evaluate(() => ['require', 'process', 'Buffer', 'global'].map(k => typeof (window as any)[k]))).toEqual(['undefined', 'undefined', 'undefined', 'undefined']);
    expect(await page.evaluate(() => Object.keys((window as any).openTouchDeck).sort())).toEqual(['onSessionChanged', 'session', 'updates', 'version']);
    expect(await page.evaluate(() => Object.keys((window as any).openTouchDeck.updates).sort())).toEqual(['check', 'download', 'install']);
    expect(await page.evaluate(() => (window as any).openTouchDeck.updates.check().catch((e: Error) => e.message))).toBe('not available yet');
    const token = await page.evaluate(() => (window as any).openTouchDeck.session());
    expect(fake.sessions.has(token)).toBe(true);
    expect(token).not.toBe(fake.runtime.token);
    await page.evaluate(() => { (window as any).changed = []; (window as any).openTouchDeck.onSessionChanged((t: string) => (window as any).changed.push(t)); });
    // UI asks again after auth.* / 401 or WebSocket 4401; the bridge has no extra refresh API.
    fake.sessions.clear();
    const renewed = await page.evaluate(() => (window as any).openTouchDeck.session());
    expect(renewed).not.toBe(token);
    expect(await page.evaluate(() => (window as any).changed)).toEqual([renewed]);
    await page.evaluate(() => { const f = document.createElement('iframe'); f.src = '/frame'; document.body.append(f); });
    await expect.poll(() => page.frames().length).toBe(2);
    expect(await page.frames()[1].evaluate(() => typeof (window as any).openTouchDeck)).toBe('undefined');
    // Exercise the actual registered handler with foreign and subframe senders.
    expect(await app.evaluate(async ({ ipcMain, BrowserWindow }) => {
      const contents = BrowserWindow.getAllWindows()[0].webContents;
      const handler = (ipcMain as any)._invokeHandlers.get('session');
      return await handler({ sender: contents, senderFrame: { origin: contents.mainFrame.origin, parent: contents.mainFrame } });
    })).toBe(null);
    expect(await app.evaluate(async ({ ipcMain, BrowserWindow }) => {
      const contents = BrowserWindow.getAllWindows()[0].webContents;
      return await (ipcMain as any)._invokeHandlers.get('session')({ sender: contents, senderFrame: { origin: 'https://example.invalid', parent: null } });
    })).toBe(null);
    await app.evaluate(({ shell }) => { (globalThis as any).opened = []; shell.openExternal = async url => { (globalThis as any).opened.push(url); }; });
    await page.evaluate(() => { location.href = 'https://example.invalid/'; });
    await expect.poll(() => app.evaluate(() => (globalThis as any).opened)).toEqual(['https://example.invalid/']);
    expect(page.url()).toBe(fake.origin + '/');
    await page.evaluate(() => window.open('https://example.invalid/popup'));
    await expect.poll(() => app.evaluate(() => (globalThis as any).opened.length)).toBe(2);
    const executable = await app.evaluate(() => process.execPath);
    const second = spawn(executable, [resolve('.')], { env, windowsHide: true, stdio: 'ignore' });
    const [code] = await once(second, 'exit');
    expect(code).toBe(0);
    expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length)).toBe(1);
    const evidence = process.env.TOUCHDECK_EVIDENCE_DIR || resolve('../../.verify/72');
    await mkdir(evidence, { recursive: true });
    await page.screenshot({ path: resolve(evidence, 'electron-shell.png') });
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].close());
    await expect.poll(() => fake.sessions.size).toBe(0);
    expect(fake.calls.some(c => c.method === 'DELETE' && c.path === '/api/v1/auth/sessions/self')).toBe(true);
  } finally { await app.close(); await fake.close(); }
});

test('wrong proof never loads a window or hands over credentials', async () => {
  const fake = await fakeCompanion({ wrongProof: true });
  // The stand-in "Companion" exits at once, so the shell keeps seeing the impostor's runtime file.
  const env = { ...process.env, TOUCHDECK_APP_DATA_DIR: fake.dir, TOUCHDECK_COMPANION_BIN: process.execPath };
  delete env.ELECTRON_RUN_AS_NODE;
  const app = await electron.launch({ args: [resolve('.')], env });
  try {
    await expect.poll(() => fake.calls.length).toBeGreaterThan(0);
    expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length)).toBe(0);
    expect(fake.calls.every(c => !c.authorization && c.path === '/api/v1/auth/proof')).toBe(true);
  } finally { await app.close(); await fake.close(); }
});
