import { join } from 'node:path';
import { connectFirstDeck } from '../deck.ts';
import { DEFAULT_KEY, start, type FixtureServer } from '../fixtures/cliproxy-server.mjs';
import { note, shot } from '../report.ts';
import { DEV_JSON, invoke, mockSets, openSources, save, saveJson, setChecked, type, until, useLayout } from '../sources.ts';

describe('sources page and layout bindings', () => {
  let server: FixtureServer;

  before(async () => {
    server = await start();
    note('port', await connectFirstDeck());
    await openSources();
    await invoke('sources_set_key', { key: DEFAULT_KEY });
  });

  after(async () => { await server.stop(); });

  it('saves the layout-file binding and reports connected feeds', async () => {
    await setChecked('proxy-enabled', true);
    await type('proxy-url', server.url);
    await useLayout(DEV_JSON);
    const layout = $('[data-testid="layout-state"]');
    expect(await layout.getAttribute('data-widgets')).toBe('4');
    expect(await layout.getAttribute('data-dashboards')).toBe('1');
    expect(await layout.getText()).toContain('4 widget tiles and 1 dashboard page');
    await until(async () => (await mockSets()).some((set) => set.tile === 'five_hour' && set.value === '42%'), 5_000, 'the layout feed');
    const feed = $('[data-testid="deck-feed"]');
    await browser.waitUntil(async () => Number(await feed.getAttribute('data-sent')) > 0, { timeout: 5_000 });
    expect(await feed.getAttribute('data-deck')).toBe('true');
    expect(await feed.getText()).toContain('Feeding the deck');
    saveJson('layout-feeds.json', { status: await invoke('sources_status'), sets: await mockSets() });
    await shot('sources');
  });

  it('reports an invalid layout and recovers after restoring the file binding', async () => {
    await type('layout-path', join(DEV_JSON, 'missing.json'));
    await save();
    const layout = $('[data-testid="layout-state"]');
    await browser.waitUntil(async () => (await layout.getAttribute('data-error')) !== '', { timeout: 5_000 });
    expect(await layout.getAttribute('data-widgets')).toBe('0');
    await useLayout(DEV_JSON);
    await invoke('disconnect');
    const feed = $('[data-testid="deck-feed"]');
    await browser.waitUntil(async () => (await feed.getAttribute('data-deck')) === 'false', { timeout: 5_000 });
    expect(await feed.getText()).toBe('No deck connected');
    await shot('sources-disconnected');
  });
});
