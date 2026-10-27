import { mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { closeConnection, openConnection, openSettings, row, waitForDot } from '../deck.ts';
import { worstCase } from '../fixtures/worst-case.ts';
import { note, out, shot } from '../report.ts';

const examples = join(import.meta.dirname, '..', '..', '..', 'schema', 'examples');
const text = (name: string) => readFileSync(join(examples, name), 'utf8');
const status = () => $('[data-testid="editor-status"]');
const preview = () => $('[data-testid="deck-preview"]');

async function importText(content: string, name: string) {
  await browser.execute(
    (body, file) => {
      const input = document.querySelector<HTMLInputElement>('[data-testid="import-file"]')!;
      const data = new DataTransfer();
      data.items.add(new File([body], file, { type: 'application/json' }));
      input.files = data.files;
      input.dispatchEvent(new Event('change', { bubbles: true }));
    },
    content,
    name,
  );
  await browser.waitUntil(async () => ['opened', 'open-failed'].includes(String(await status().getAttribute('data-event'))), { timeout: 5_000 });
  await browser.pause(250);
}

async function waitEvent(event: string) {
  await browser.waitUntil(async () => (await status().getAttribute('data-event')) === event, {
    timeout: 5_000,
    timeoutMsg: `the editor never reported ${event}: ${await status().getAttribute('data-event')}`,
  });
}

/** Swipes the deck screen sideways by `dx` deck pixels, with the pointer events a finger makes. */
async function swipe(dx: number) {
  await browser.execute((distance) => {
    const screen = document.querySelector<HTMLElement>('[data-testid="deck-screen"]')!;
    const box = screen.getBoundingClientRect();
    const scale = Number(screen.dataset.scale);
    const at = (x: number) => ({ clientX: box.left + x * scale, clientY: box.top + 300 * scale });
    const pointer = (type: string, x: number) =>
      new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: 1, pointerId: 1, isPrimary: true, pointerType: 'touch', ...at(x) });
    screen.dispatchEvent(pointer('pointerdown', 600));
    for (let step = 1; step <= 6; step++) window.dispatchEvent(pointer('pointermove', 600 + (distance * step) / 6));
    window.dispatchEvent(pointer('pointerup', 600 + distance));
  }, dx);
  await browser.pause(400);
}

async function tap(id: string) {
  await browser.execute((tile) => {
    const box = document.querySelector<HTMLElement>(`[data-testid="tile"][data-id="${tile}"]`)!;
    const rect = box.getBoundingClientRect();
    const point = { clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2 };
    const pointer = (type: string) => new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: 1, pointerId: 1, isPrimary: true, pointerType: 'touch', ...point });
    box.dispatchEvent(pointer('pointerdown'));
    window.dispatchEvent(pointer('pointerup'));
  }, id);
  await browser.pause(400);
}

/** A window screenshot and where the deck screen sits in it, in device pixels, so the screen can be cut out at 1:1. */
async function deckShot(name: string) {
  mkdirSync(out, { recursive: true });
  const rect = await browser.execute(() => {
    const box = document.querySelector('[data-testid="deck-screen"]')!.getBoundingClientRect();
    const r = devicePixelRatio;
    return { x: Math.round(box.left * r), y: Math.round(box.top * r), w: Math.round(box.width * r), h: Math.round(box.height * r) };
  });
  await browser.saveScreenshot(join(out, `${name}-window.png`));
  note(name, rect);
}

describe('the app, screen by screen, on the mock deck', () => {
  before(async () => {
    await browser.setWindowSize(1180, 760);
  });

  it('greets a first-time person who has not connected yet', async () => {
    await $('[data-testid="welcome"]').waitForExist({ timeout: 5_000 });
    await $('[data-testid="connect-banner"]').waitForExist({ timeout: 5_000 });
    expect(await $('[data-testid="status-dot"]').getAttribute('data-state')).toBe('offline');
    expect(await $('[data-testid="link-status"]').getText()).toBe('Deck not connected');
    await shot('first-run');
  });

  it('lists the deck under the connection indicator and connects', async () => {
    await openConnection();
    await row('mock').waitForExist({ timeout: 15_000 });
    await shot('connection-panel');
    await row('mock').$('[data-testid="connect"]').click();
    await waitForDot('online', 5_000);
    expect(await $('[data-testid="link-status"]').getText()).toBe('Deck connected');
    await closeConnection();
  });

  it('starts with examples and edits on a canvas drawn like the deck, in landscape', async () => {
    await $('[data-testid="empty-starter"]').click();
    await waitEvent('starter');
    expect(await preview().getAttribute('data-orientation')).toBe('landscape');
    expect(await $('[data-testid="deck-screen"]').getAttribute('style')).toContain('width: 1024px; height: 600px');
    await shot('home-landscape');
    await $('[data-testid="tile"][data-id="playpause"]').click();
    await $('[data-testid="tile-props"][data-id="playpause"]').waitForExist({ timeout: 3_000 });
    await shot('home-button-selected');
  });

  it('keeps a separate layout for portrait', async () => {
    await $('[data-testid="orientation-portrait"]').click();
    await $('[data-testid="welcome"]').waitForExist({ timeout: 3_000 });
    await $('[data-testid="empty-starter"]').click();
    await waitEvent('starter');
    expect(await preview().getAttribute('data-orientation')).toBe('portrait');
    expect(await $('[data-testid="deck-screen"]').getAttribute('style')).toContain('width: 600px; height: 1024px');
    await $('[data-testid="page-tab"][data-id="shortcuts"]').click();
    await shot('home-portrait');
    await $('[data-testid="orientation-landscape"]').click();
  });

  it('tries pages out like the deck: swipe, tap, and a page jump', async () => {
    await importText(text('dev.json'), 'dev.json');
    await $('[data-testid="mode-preview"]').click();
    await preview().waitForExist();
    expect(await preview().getAttribute('data-mode')).toBe('preview');
    expect(await $('[data-testid="tile-props"]').isExisting()).toBe(false);
    await shot('preview-mode');
    await swipe(-200);
    expect(await preview().getAttribute('data-page')).toBe('desktop');
    await swipe(300);
    expect(await preview().getAttribute('data-page')).toBe('shortcuts');
    await tap('accounts_jump');
    expect(await preview().getAttribute('data-page')).toBe('accounts');
    await shot('preview-dashboard');
    note('preview', { swiped: true, jumped: 'accounts' });
  });

  it('uses the whole screen when asked, and goes back', async () => {
    await $('[data-testid="mode-edit"]').click();
    await $('[data-testid="page-tab"][data-id="shortcuts"]').click();
    await $('[data-testid="full-screen"]').click();
    await browser.waitUntil(async () => (await preview().getAttribute('data-full-screen')) === 'true', { timeout: 3_000 });
    expect(await $('[data-testid="deck-title"]').isExisting()).toBe(false);
    expect(await $$('[data-testid="deck-dot"]').length).toBe(0);
    await shot('fullscreen-on');
    await $('[data-testid="mode-preview"]').click();
    await shot('fullscreen-on-preview');
    await $('[data-testid="mode-edit"]').click();
    await $('[data-testid="full-screen"]').click();
    await browser.waitUntil(async () => (await preview().getAttribute('data-full-screen')) === 'false', { timeout: 3_000 });
    expect(await $$('[data-testid="deck-dot"]').length).toBe(4);
    await shot('fullscreen-off');
  });

  it('explains a refused change and a file it cannot open', async () => {
    await importText('{"schema": 1, "name": "Broken"', 'broken.json');
    expect(await status().getAttribute('data-kind')).toBe('error');
    expect(await status().getText()).toContain("isn't a TouchDeck layout file");
    await status().$('summary').click();
    await shot('error-state');
  });

  it('shows the settings, AI usage, and troubleshooting screens', async () => {
    await openSettings('deck');
    await browser.pause(300);
    await shot('settings');
    await openSettings('help');
    await shot('troubleshooting');
    await $('[data-testid="nav-usage"]').click();
    await $('[data-testid="sources-page"]').waitForExist({ timeout: 5_000 });
    await shot('usage');
    await $('[data-testid="nav-buttons"]').click();
  });

  it('holds up to the worst realistic layout', async () => {
    await importText(JSON.stringify(worstCase), 'worst-case.json');
    await waitEvent('opened');
    await shot('worst-case-edit');
    await $('[data-testid="tile"][data-id="t4"]').click();
    await shot('worst-case-selected');
    await $('[data-testid="mode-preview"]').click();
    await shot('worst-case-preview');
    for (const id of ['column', 'accounts', 'meters', 'media', 'empty']) {
      await $(`[data-testid="page-tab"][data-id="${id}"]`).click();
      await browser.pause(350);
      await shot(`worst-case-${id}`);
    }
    await $('[data-testid="mode-edit"]').click();
    await browser.setWindowSize(960, 600);
    await $('[data-testid="page-tab"][data-id="full"]').click();
    await browser.pause(300);
    const layout = await browser.execute(() => ({ scrollWidth: document.documentElement.scrollWidth, clientWidth: document.documentElement.clientWidth }));
    note('worst_small', layout);
    expect(layout.scrollWidth).toBeLessThanOrEqual(layout.clientWidth);
    await shot('worst-case-small-window');
    await browser.setWindowSize(1180, 760);
    await importText(JSON.stringify({ ...worstCase, orientation: 'portrait' }), 'worst-case-portrait.json');
    await shot('worst-case-portrait');
    await $('[data-testid="orientation-landscape"]').click();
  });

  it('draws the built-in pages at one deck pixel per screen pixel', async () => {
    await browser.setWindowSize(1500, 1000);
    await importText(text('builtin.json'), 'builtin.json');
    await $('[data-testid="mode-preview"]').click();
    await $('[data-testid="actual-size"]').click();
    await browser.pause(400);
    await deckShot('preview-1to1-fkeys');
    await swipe(-200);
    await deckShot('preview-1to1-media');
    const size = await browser.execute(() => {
      const box = document.querySelector('[data-testid="deck-screen"]')!.getBoundingClientRect();
      return { w: box.width * devicePixelRatio, h: box.height * devicePixelRatio, dpr: devicePixelRatio };
    });
    note('one_to_one', size);
    expect(Math.round(size.w)).toBe(1024);
    expect(Math.round(size.h)).toBe(600);
    await $('[data-testid="actual-size"]').click();
    await $('[data-testid="mode-edit"]').click();
    await browser.setWindowSize(1180, 760);
  });
});
