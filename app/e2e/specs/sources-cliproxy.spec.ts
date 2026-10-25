import { mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectFirstDeck } from '../deck.ts';
import { note, shot } from '../report.ts';
import { DEFAULT_KEY, start, type FixtureServer } from '../fixtures/cliproxy-server.mjs';
import { DEV_JSON, invoke, mockSets, openSources, saveJson, saveText, setChecked, save, type, until, useLayout, type Set } from '../sources.ts';

const ACCOUNTS = ['ada@example.com', 'grace@example.com', 'linus@example.com'];
const dashboards = (sets: Set[]) => sets.filter((set) => set.m === 'dashboard.set');
const tile = (sets: Set[], id: string) => sets.filter((set) => set.m === 'widget.set' && set.tile === id);

// One layout with a widget tile per account, for the per-account half of the dashboard lane.
function accountLayout() {
  const dir = join(tmpdir(), `td-sources-spec-${process.pid}`);
  mkdirSync(dir, { recursive: true });
  const tiles = ACCOUNTS.map((email, col) => ({
    id: email.split('@')[0],
    label: email.split('@')[0],
    row: 0,
    col,
    rowSpan: 1,
    colSpan: 1,
    icon: null,
    action: { type: 'widget', source: 'cliproxyapi', field: 'recent_10m', account: email },
  }));
  const layout = {
    schema: 1,
    name: 'Accounts',
    orientation: 'landscape',
    pages: [
      { id: 'tiles', title: 'Tiles', kind: 'grid', rows: 1, cols: 3, tiles },
      { id: 'accounts', title: 'Accounts', kind: 'dashboard', source: 'cliproxyapi' },
    ],
  };
  const path = join(dir, 'accounts.json');
  writeFileSync(path, JSON.stringify(layout));
  return path;
}

describe('cliproxyapi source', () => {
  let server: FixtureServer;

  before(async () => {
    server = await start({ key: DEFAULT_KEY });
    note('server', server.url);
    note('port', await connectFirstDeck());
    await openSources();
  });

  after(async () => {
    await server.stop();
  });

  it('keeps the management key out of the page and shows its last four characters', async () => {
    await type('proxy-key', DEFAULT_KEY);
    await $('[data-testid="proxy-key-save"]').click();
    await browser.waitUntil(async () => (await $('[data-testid="proxy-key-hint"]').getText()) === `…${DEFAULT_KEY.slice(-4)}`, {
      timeout: 5_000,
      timeoutMsg: 'the key hint did not show the last 4 characters',
    });
    expect(await $('[data-testid="proxy-key"]').getValue()).toBe('');
    expect(await $('body').getText()).not.toContain(DEFAULT_KEY);
  });

  it('sends one dashboard.set with a row per account and a widget.set for the proxy tile', async () => {
    await setChecked('proxy-enabled', true);
    await type('proxy-url', server.url);
    await type('proxy-interval', '300');
    await useLayout(DEV_JSON);
    const sets = await until(
      async () => {
        const all = await mockSets();
        return dashboards(all).length > 0 && tile(all, 'proxy').length > 0 && all;
      },
      5_000,
      'the dashboard.set and the proxy widget.set',
    );
    const dashboard = dashboards(sets);
    saveJson('dashboard-set.json', { dashboard_sets: dashboard, proxy_widget_sets: tile(sets, 'proxy') });
    expect(dashboard).toHaveLength(1);
    expect(dashboard[0].page).toBe('accounts');
    expect(dashboard[0].rows?.map((row) => row.label)).toEqual(ACCOUNTS.map((email) => email.slice(0, 16)));
    expect(dashboard[0].rows?.map((row) => row.value)).toEqual(['active', 'active', 'error']);
    expect(dashboard[0].rows?.map((row) => row.bar)).toEqual([5 / 12, 1, 0]);
    expect(tile(sets, 'proxy').at(-1)).toMatchObject({ value: 'error', stale: false });
    await $('[data-testid="proxy-state"][data-state="ok"]').waitForExist({ timeout: 5_000 });
    expect(await $$('[data-testid="proxy-account"]').length).toBe(3);
    await shot('cliproxy');
  });

  it('sends a widget.set per account tile for a layout with one', async () => {
    await useLayout(accountLayout());
    const sets = await until(
      async () => {
        const all = await mockSets();
        return ACCOUNTS.every((email) => tile(all, email.split('@')[0]).length > 0) && all;
      },
      5_000,
      'a widget.set per account tile',
    );
    const values = Object.fromEntries(ACCOUNTS.map((email) => [email, tile(sets, email.split('@')[0]).at(-1)]));
    note('account_tiles', values);
    expect(values['ada@example.com']).toMatchObject({ value: '5', bar: 5 / 12 });
    expect(values['grace@example.com']).toMatchObject({ value: '12', bar: 1 });
    expect(values['linus@example.com']).toMatchObject({ value: '0', bar: 0 });
  });

  it('polls once, at once, when a CLIProxyAPI tile is tapped', async () => {
    await useLayout(DEV_JSON);
    await browser.pause(1_500);
    const before = server.requests.length;
    const pressed = Date.now();
    await invoke('e2e_mock_press', { page: 'usage', tile: 'proxy' });
    const request = await until(async () => server.requests[before], 1_000, 'a new auth-files request');
    await browser.pause(1_500);
    const after = server.requests.slice(before);
    saveText(
      'tap-refresh.txt',
      [`pressed_at ${pressed}`, ...server.requests.map((r, n) => `${n >= before ? 'after ' : 'before'} ${r.at} ${r.method} ${r.path} authorized=${r.authorized}`), `delay_ms ${request.at - pressed}`, `requests_after_press ${after.length}`, ''].join('\n'),
    );
    note('tap_delay_ms', request.at - pressed);
    expect(request.at - pressed).toBeLessThan(1_000);
    expect(after).toHaveLength(1);
    expect(request).toMatchObject({ method: 'GET', path: '/v0/management/auth-files', authorized: true });
    await invoke('e2e_mock_press', { page: 'shortcuts', tile: 'f5' });
    await browser.pause(800);
    expect(server.requests.length - before).toBe(1);
  });

  it('marks the proxy tile stale after three failed polls and clears it when the server returns', async () => {
    await type('proxy-interval', '2');
    await save();
    await browser.pause(2_500);
    const port = server.port;
    const stopped = Date.now();
    await server.stop();
    const trace: { t: number; fails: number; stale_sent: boolean }[] = [];
    const staleSent = async () => tile(await mockSets(), 'proxy').some((set) => set.stale === true && set.at_ms >= stopped);
    await until(
      async () => {
        const fails = Number(await $('[data-testid="proxy-state"]').getAttribute('data-fails'));
        const stale_sent = await staleSent();
        trace.push({ t: Date.now() - stopped, fails, stale_sent });
        return stale_sent && fails >= 3;
      },
      20_000,
      'stale true after the third failure',
    );
    const early = trace.filter((sample) => sample.fails < 3 && sample.stale_sent);
    const stale = tile(await mockSets(), 'proxy').find((set) => set.stale === true && set.at_ms >= stopped)!;
    const dashboardStale = dashboards(await mockSets()).at(-1)!;
    await shot('proxy-stale');

    server = await start({ port, key: DEFAULT_KEY });
    const returned = Date.now();
    const cleared = await until(async () => tile(await mockSets(), 'proxy').find((set) => set.stale === false && set.at_ms > stale.at_ms), 10_000, 'stale false after the server returned');
    saveJson('three-fails.json', { stale_after_ms: stale.at_ms - stopped, early_stale_samples: early, cleared_after_ms: cleared.at_ms - returned, trace, dashboard_when_stale: dashboardStale });
    note('stale_after_ms', stale.at_ms - stopped);
    note('cleared_after_ms', cleared.at_ms - returned);
    expect(early).toEqual([]);
    expect(dashboardStale.rows?.every((row) => row.value === 'stale')).toBe(true);
    expect(cleared.at_ms - returned).toBeLessThan(3_500);
    await $('[data-testid="proxy-state"][data-state="ok"]').waitForExist({ timeout: 5_000 });
  });
});
