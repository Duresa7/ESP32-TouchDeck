import { expect, test } from '@playwright/test';
import { created, openCopyTile, openPicker, option, picker, searches, storedTile } from './icon-picker-helpers.ts';

type Idle = { reset(): void; read(): { timeouts: number; frames: number; intervals: number; live: number }; live(): string[] };

declare global {
  interface Window {
    idle?: Idle;
    inFlight?: () => number;
  }
}

const PNG = Buffer.from('89504e470d0a1a0a0000000d4948445200000001000000010806000000', 'hex');

test('typing fast sends one search, and a picked logo becomes the Tile’s Asset', async ({ page }) => {
  await openCopyTile(page);
  await openPicker(page);
  await page.getByTestId('icon-search').pressSequentially('spotify', { delay: 20 });
  await expect(option(page, 'hit:simple-icons:spotify')).toBeVisible();
  expect((await searches(page)).map((s) => s.q)).toEqual(['spotify']);

  await option(page, 'hit:simple-icons:spotify').click();
  await expect(picker(page)).toBeHidden();
  await expect(page.getByTestId('tile-icon-name')).toHaveText('Spotify');
  const sources = await created(page);
  expect(sources).toEqual([{ kind: 'catalog', catalog: 'simple-icons', id: 'spotify', variant: 'base', padding: 0, background: null, tint: [0x1e, 0xd7, 0x60] }]);
  const stored = await storedTile(page, 'copy');
  expect(stored.schema).toBe(2);
  expect(stored.tile?.icon).toBeNull();
  expect(stored.tile?.asset).toMatch(/^sha256:[0-9a-f]{64}$/);
  await expect(page.getByTestId('tile-icon').locator('canvas[data-ready="true"]')).toBeVisible();
});

test('recent picks come first, newest first', async ({ page }) => {
  await openCopyTile(page);
  await openPicker(page);
  await option(page, 'lv:mute').click();
  await openPicker(page);
  await page.getByTestId('icon-search').fill('discord');
  await option(page, 'hit:simple-icons:discord').click();
  await expect(picker(page)).toBeHidden();
  await openPicker(page);
  const sections = picker(page).locator('[data-testid^="icon-section-"]');
  await expect(sections.first()).toHaveAttribute('data-testid', 'icon-section-recent');
  const recent = picker(page).getByTestId('icon-section-recent').getByTestId('icon-option');
  await expect(recent).toHaveCount(2);
  await expect(recent.nth(0)).toHaveAccessibleName('Discord');
  await expect(recent.nth(1)).toHaveAccessibleName('Speaker off');
  await expect(recent.nth(0)).toHaveAttribute('aria-current', 'true');
});

test('a recent that no longer resolves drops out quietly', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('touchdeck.recent-icons', JSON.stringify([{ kind: 'asset', asset: 'f'.repeat(64), name: 'Gone' }, { kind: 'symbol', icon: 'lv:next', name: 'Next' }]));
  });
  await openCopyTile(page);
  await openPicker(page);
  const recent = picker(page).getByTestId('icon-section-recent').getByTestId('icon-option');
  await expect(recent).toHaveCount(1);
  await expect(recent.first()).toHaveAccessibleName('Next');
  await expect(picker(page).getByTestId('icon-error')).toHaveCount(0);
  expect(await page.evaluate(() => localStorage.getItem('touchdeck.recent-icons'))).not.toContain('Gone');
});

test('uploading an image still sets the Tile’s icon', async ({ page }) => {
  await openCopyTile(page);
  await openPicker(page);
  await page.getByTestId('icon-upload-file').setInputFiles({ name: 'Ünïcödé logo.png', mimeType: 'image/png', buffer: PNG });
  await expect(picker(page)).toBeHidden();
  await expect(page.getByTestId('tile-icon-name')).toHaveText('Ünïcödé logo.png');
  expect((await created(page))[0]?.kind).toBe('upload');
  expect((await storedTile(page, 'copy')).tile?.asset).toMatch(/^sha256:/);
});

test('a render failure explains itself and retries', async ({ page }) => {
  await openCopyTile(page);
  await page.evaluate(() => window.touchdeckDemo?.assets.failRender('spotify'));
  await openPicker(page);
  await page.getByTestId('icon-search').fill('spotify');
  await option(page, 'hit:simple-icons:spotify').click();
  const error = picker(page).getByTestId('icon-error');
  await expect(error).toContainText("Couldn't get Spotify. Check the internet connection, then retry.");
  await error.locator('summary').click();
  await expect(error).toContainText('Icon download failed');
  await picker(page).getByTestId('icon-retry').click();
  await expect(error).toBeVisible();
  expect(await created(page)).toHaveLength(2);
  await expect(page.getByTestId('tile-icon-name')).toHaveText('No icon');
});

test('an offline Companion, no results and an empty search each say what to do', async ({ page }) => {
  await openCopyTile(page);
  await openPicker(page);
  await expect(picker(page)).toContainText('Search Simple Icons and selfh.st logos, or pick a built-in icon.');
  await page.getByTestId('icon-search').fill('zzzqqq');
  await expect(picker(page).getByTestId('icon-status')).toHaveText('No icons match “zzzqqq”. Try a shorter word.');
  await page.evaluate(() => window.touchdeckDemo?.assets.offline(true));
  await page.getByTestId('icon-search').fill('plex');
  await expect(picker(page).getByTestId('icon-search-error')).toContainText("Can't reach the Companion. Check that Open TouchDeck is running in the tray, then retry.");
  await page.evaluate(() => window.touchdeckDemo?.assets.offline(false));
  await picker(page).getByTestId('icon-search-error').getByRole('button', { name: 'Retry' }).click();
  await expect(option(page, 'hit:simple-icons:plex')).toBeVisible();
});

test('works from the keyboard alone', async ({ page }) => {
  await openCopyTile(page);
  await page.getByTestId('icon-change').focus();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('icon-search')).toBeFocused();
  await page.keyboard.type('steam');
  await expect(option(page, 'hit:simple-icons:steam')).toBeVisible();
  await page.keyboard.press('ArrowDown');
  const first = picker(page).getByTestId('icon-section-logos').getByTestId('icon-option').first();
  await expect(first).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(picker(page).getByTestId('icon-section-logos').getByTestId('icon-option').nth(1)).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('ArrowUp');
  await expect(page.getByTestId('icon-search')).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await expect(picker(page)).toBeHidden();
  await expect(page.getByTestId('icon-change')).toBeFocused();
  await expect(page.getByTestId('tile-icon-name')).toHaveText('Steam');

  await page.keyboard.press('Enter');
  await expect(page.getByTestId('icon-search')).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(picker(page)).toBeHidden();
  await expect(page.getByTestId('icon-change')).toBeFocused();
});

test('closing aborts the search in flight, and a closed picker runs no timers', async ({ page }) => {
  await page.addInitScript(() => {
    const counts = { timeouts: 0, frames: 0, intervals: 0, live: 0 };
    const live = new Map<number, string>();
    const timeout = window.setTimeout.bind(window);
    const interval = window.setInterval.bind(window);
    const clear = window.clearInterval.bind(window);
    const frame = window.requestAnimationFrame.bind(window);
    Object.assign(window, {
      setTimeout: (handler: TimerHandler, ms?: number, ...args: unknown[]) => {
        counts.timeouts += 1;
        return timeout(handler, ms, ...args);
      },
      setInterval: (handler: TimerHandler, ms?: number, ...args: unknown[]) => {
        counts.intervals += 1;
        const id = interval(handler, ms, ...args);
        live.set(id, `${ms} ms ${new Error().stack?.split('\n')[2]?.trim() ?? ''}`);
        return id;
      },
      clearInterval: (id?: number) => {
        if (id !== undefined) live.delete(id);
        clear(id);
      },
      requestAnimationFrame: (callback: FrameRequestCallback) => {
        counts.frames += 1;
        return frame(callback);
      },
      idle: {
        reset: () => Object.assign(counts, { timeouts: 0, frames: 0, intervals: 0 }),
        read: () => ({ ...counts, live: live.size }),
        live: () => [...live.values()],
      },
    });
  });
  await openCopyTile(page);
  await page.evaluate(() => {
    const through = window.fetch;
    let inFlight = 0;
    window.fetch = async (input, init) => {
      inFlight += 1;
      try {
        return await through(input, init);
      } finally {
        inFlight -= 1;
      }
    };
    Object.assign(window, { inFlight: () => inFlight });
  });
  const resetIdle = () => page.evaluate(() => window.idle?.reset());
  const readIdle = () =>
    page.evaluate(() => {
      if (!window.idle) throw new Error('the timer probe is missing');
      return window.idle.read();
    });
  const inFlight = () => page.evaluate(() => window.inFlight?.() ?? -1);

  await page.mouse.move(0, 0);
  await page.waitForTimeout(500);
  await resetIdle();
  await page.waitForTimeout(2000);
  const before = await readIdle();

  await page.evaluate(() => window.touchdeckDemo?.assets.holdSearches(true));
  await openPicker(page);
  await page.getByTestId('icon-search').fill('youtube');
  await expect.poll(async () => (await searches(page)).length).toBe(1);
  expect(await inFlight()).toBe(1);
  await page.keyboard.press('Escape');
  await expect(picker(page)).toHaveCount(0);
  await page.waitForTimeout(400);
  expect(await inFlight()).toBe(0);
  const asked = await searches(page);
  expect(asked.map((s) => s.q)).toEqual(['youtube']);
  expect(asked[0].aborted).toBe(true);

  await resetIdle();
  await page.waitForTimeout(2000);
  const after = await readIdle();
  const intervals = await page.evaluate(() => window.idle?.live() ?? []);
  test.info().annotations.push({ type: 'idle', description: JSON.stringify({ before, after, intervals }) });
  expect(after).toEqual(before);
  expect({ timeouts: after.timeouts, frames: after.frames, intervals: after.intervals }).toEqual({ timeouts: 0, frames: 0, intervals: 0 });
});
