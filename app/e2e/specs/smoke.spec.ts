import { execFileSync } from 'node:child_process';
import { HELLO_FIELDS, MOCK_ELF_SHA256, openConnection, openSettings, row, waitForDot } from '../deck.ts';
import { note, shot } from '../report.ts';

describe('smoke on the mock deck', () => {
  it('names the product in the window title and header', async () => {
    await browser.waitUntil(async () => (await browser.getTitle()) === 'Open TouchDeck', {
      timeout: 5_000,
      timeoutMsg: 'title is not "Open TouchDeck"',
    });
    expect(await $('[data-testid="title"] span').getText()).toBe('Open TouchDeck');
    const nativeTitle = await browser.execute(async () => {
      const tauri = (window as unknown as { __TAURI__: { window: { getCurrentWindow: () => { title: () => Promise<string> } } } }).__TAURI__;
      return tauri.window.getCurrentWindow().title();
    });
    expect(nativeTitle).toBe('Open TouchDeck');
    note('native_title', nativeTitle);
    note('title', await browser.getTitle());
  });

  it('lists the mock deck', async () => {
    await openConnection();
    await row('mock').waitForExist({ timeout: 10_000 });
    expect(await row('mock').getAttribute('data-status')).toBe('deck');
  });

  it('turns the status dot green on Connect', async () => {
    expect(await $('[data-testid="status-dot"]').getAttribute('data-state')).toBe('offline');
    await row('mock').$('[data-testid="connect"]').click();
    await waitForDot('online', 5_000);
    expect(await $('[data-testid="link-status"]').getText()).toBe('Deck connected');
  });

  it('keeps the build commit in Settings', async () => {
    await openSettings('help');
    const build = $('[data-testid="app-build"]');
    await build.waitForDisplayed({ timeout: 5_000 });
    const sha = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
    expect(await build.getText()).toBe(sha);
    expect(await browser.getTitle()).toBe('Open TouchDeck');
    note('app_sha', await build.getText());
    await shot('app-build');
  });

  it('renders the hello fields', async () => {
    await openSettings('help');
    await $('[data-testid="deck-info"]').waitForExist({ timeout: 5_000 });
    for (const field of HELLO_FIELDS) {
      expect(await $(`[data-testid="hello-${field}"] dd`).getText()).not.toBe('');
    }
    expect(await $('[data-testid="hello-product"] dd').getText()).toBe('touchdeck');
    expect(await $('[data-testid="hello-proto"] dd').getText()).toBe('1');
    expect(await $('[data-testid="hello-elf_sha256"] dd').getText()).toBe(MOCK_ELF_SHA256);
    const marks = await browser.execute(() => {
      const at = (name: string) => performance.getEntriesByName(name)[0]?.startTime ?? null;
      return { origin: performance.timeOrigin, list_ready: at('list-ready'), connect_start: at('connect-start'), connected: at('connected') };
    });
    const spawned = Number(process.env.TD_E2E_SPAWNED_AT ?? NaN);
    note('marks', marks);
    note('perf', {
      start_ms: marks.list_ready === null || Number.isNaN(spawned) ? null : Math.round(marks.origin + marks.list_ready - spawned),
      connect_ms: marks.connected === null || marks.connect_start === null ? null : Math.round(marks.connected - marks.connect_start),
    });
    await shot('smoke');
  });
});
