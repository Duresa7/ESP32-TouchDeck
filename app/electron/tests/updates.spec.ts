import { test, expect, _electron as electron } from '@playwright/test';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fakeCompanion } from './fake-companion.mjs';
import type {} from '../../ui/src/companion/session';

const kinds = [['legacy', 'Ed'], ['prehashed', 'ED']] as const;

for (const [kind, algorithm] of kinds) test(`authorized top frame reaches check download and install handlers (${algorithm} ${kind})`, async () => {
  const fixture: unknown = JSON.parse(await readFile(resolve('dist/e2e-update.json'), 'utf8'));
  if (!fixture || typeof fixture !== 'object' || !('data' in fixture) || typeof fixture.data !== 'string' ||
      !('signatures' in fixture) || !fixture.signatures || typeof fixture.signatures !== 'object' ||
      !(kind in fixture.signatures)) throw new Error('Invalid test fixture');
  const signature: unknown = fixture.signatures[kind];
  if (typeof signature !== 'string') throw new Error('Invalid test fixture');
  const bytes = Buffer.from(fixture.data, 'base64');
  let origin = '';
  const feed = createServer((req, res) => {
    if (req.url === '/latest.json') res.end(JSON.stringify({ version: '99.0.0', platforms: {
      'windows-x86_64': { url: `${origin}/setup.exe`, signature },
    } }));
    else res.end(bytes);
  });
  await new Promise<void>(resolve => feed.listen(0, '127.0.0.1', resolve));
  const address = feed.address();
  if (!address || typeof address === 'string') throw new Error('Feed unavailable');
  origin = `http://127.0.0.1:${address.port}`;
  const fake = await fakeCompanion({ updateBusy: true });
  const env = { ...process.env, TOUCHDECK_APP_DATA_DIR: fake.dir, TOUCHDECK_UPDATE_TEST_FEED: `${origin}/latest.json` };
  delete env.ELECTRON_RUN_AS_NODE;
  const app = await electron.launch({ args: [resolve('.')], env });
  try {
    const page = await app.firstWindow();
    await page.waitForURL(fake.origin + '/');
    expect(await page.evaluate(() => window.openTouchDeck?.updates?.check())).toMatchObject({ state: 'available', version: '99.0.0' });
    expect(await page.evaluate(() => window.openTouchDeck?.updates?.download(() => {}))).toEqual({ state: 'ready' });
    expect(await page.evaluate(() => window.openTouchDeck?.updates?.install())).toEqual({ state: 'failed', reason: 'busy', device: 'Studio' });
    expect(fake.calls.some(call => call.path === '/api/v1/system/lock')).toBe(true);
  } finally {
    await app.close(); await fake.close();
    feed.closeAllConnections(); await new Promise<void>(resolve => feed.close(() => resolve()));
  }
});
