import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectFirstDeck } from '../deck.ts';
import { note, out } from '../report.ts';
import { API_CALL, AUTH_FILES, CLAUDE_USAGE, CODEX_USAGE, DEFAULT_KEY, start, type FixtureServer } from '../fixtures/cliproxy-server.mjs';
import { DEV_JSON, invoke, mockSets, openSources, saveJson, saveText, setChecked, save, type, until, useLayout, type Set } from '../sources.ts';
import { PROXY_FIELDS, type SourcesView, type Status } from '../../src/lib/sources/api.ts';

const ACCOUNTS = ['codex:alice@example.com', 'claude:alice@example.com', 'gemini-cli:linus@example.com'];
const accountId = (account: string) => account.split(':')[0].replaceAll('-', '_');
let lastRefresh = 0;
async function afterRefreshFloor() {
  const wait = lastRefresh + 15_100 - Date.now();
  if (wait > 0) await browser.pause(wait);
}
async function cardShot(name: string) {
  const card = $('[data-testid="proxy-card"]');
  await card.scrollIntoView();
  mkdirSync(out, { recursive: true });
  await card.saveScreenshot(join(out, `${name}.png`));
}
// The app's embedded WebDriver does not dispatch change when clicking an option.
// Use the editor spec's event-driving convention for native selects.
async function choose(testid: string, value: string) {
  await browser.execute((id, next) => {
    const select = document.querySelector(`[data-testid="${id}"]`);
    if (!(select instanceof HTMLSelectElement)) throw new Error(`missing select: ${id}`);
    select.value = next;
    select.dispatchEvent(new Event('change', { bubbles: true }));
  }, testid, value);
  await browser.waitUntil(async () => (await $(`[data-testid="${testid}"]`).getValue()) === value, { timeout: 5_000 });
}
const dashboards = (sets: Set[]) => sets.filter((set) => set.m === 'dashboard.set');
const tile = (sets: Set[], id: string) => sets.filter((set) => set.m === 'widget.set' && set.tile === id);

// One layout with a widget tile per account, for the per-account half of the dashboard lane.
function accountLayout() {
  const dir = join(tmpdir(), `td-sources-spec-${process.pid}`);
  mkdirSync(dir, { recursive: true });
  const tiles = ACCOUNTS.map((account, col) => ({
    id: accountId(account),
    label: accountId(account),
    row: 0,
    col,
    rowSpan: 1,
    colSpan: 1,
    icon: null,
    action: { type: 'widget', source: 'cliproxyapi', field: col === 0 ? 'five_hour_pct' : 'recent_10m', account },
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
    expect(dashboard[0].rows?.map((row) => row.label)).toEqual(['codex alice', 'claude alice', 'gemini-cli linus', 'codex disabled']);
    expect(dashboard[0].rows?.map((row) => row.value)).toEqual(['5h 42% 7d 10%', '5h 24% 7d 61%', 'error', 'disabled']);
    expect(dashboard[0].rows?.map((row) => row.bar)).toEqual([0.42, 0.61, 0, 0]);
    expect(tile(sets, 'proxy').at(-1)).toMatchObject({ value: 'error', stale: false });
    await $('[data-testid="proxy-state"][data-state="ok"]').waitForExist({ timeout: 5_000 });
    expect(await $$('[data-testid="proxy-account"]').length).toBe(4);
    const codex = $('[data-account-key="codex:alice@example.com"]');
    expect(await codex.$('[data-testid="proxy-account-plan"]').getText()).toBe('pro');
    expect(await codex.$('[data-testid="proxy-account-five-hour"]').getText()).toContain('42%');
    expect(await codex.$('[data-testid="proxy-account-five-hour"]').getText()).toContain('resets in 2h');
    expect(await codex.$('[data-testid="proxy-account-seven-day"]').getText()).toContain('10%');
    expect(await $('[data-account-key="claude:alice@example.com"] [data-testid="proxy-account-seven-day"]').getText()).toContain('61%');
    expect(await $('[data-testid="proxy-http-warning"]').isExisting()).toBe(false);
    expect(await $('[data-testid="proxy-interval"]').getAttribute('min')).toBe('60');
    await cardShot('cliproxy');
    await codex.$('[data-testid="proxy-account-copy"]').click();
    await browser.waitUntil(async () => (await codex.$('[data-testid="proxy-account-copy"]').getText()) === 'Copied', { timeout: 5_000 });
  });

  it('sends a widget.set per account tile for a layout with one', async () => {
    await useLayout(accountLayout());
    const sets = await until(
      async () => {
        const all = await mockSets();
        return ACCOUNTS.every((account) => tile(all, accountId(account)).length > 0) && all;
      },
      5_000,
      'a widget.set per account tile',
    );
    const values = Object.fromEntries(ACCOUNTS.map((account) => [account, tile(sets, accountId(account)).at(-1)]));
    note('account_tiles', values);
    expect(values['codex:alice@example.com']).toMatchObject({ value: '42%', bar: 0.42 });
    expect(values['claude:alice@example.com']).toMatchObject({ value: '5', bar: 5 / 12 });
    expect(values['gemini-cli:linus@example.com']).toMatchObject({ value: '0', bar: 0 });
  });

  it('polls once, at once, when a CLIProxyAPI tile is tapped', async () => {
    await useLayout(DEV_JSON);
    await browser.pause(1_500);
    const before = server.requests.length;
    await afterRefreshFloor();
    const pressed = Date.now();
    lastRefresh = pressed;
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
    expect(after).toHaveLength(3);
    expect(after.filter((r) => r.path === AUTH_FILES)).toHaveLength(1);
    expect(after.filter((r) => r.path === API_CALL)).toHaveLength(2);
    await $('[data-testid="proxy-refresh"]').click();
    await invoke('e2e_mock_press', { page: 'usage', tile: 'proxy' });
    expect(request).toMatchObject({ method: 'GET', path: '/v0/management/auth-files', authorized: true });
    await invoke('e2e_mock_press', { page: 'shortcuts', tile: 'f5' });
    await browser.pause(800);
    expect(server.requests.length - before).toBe(3);
  });

  it('uses only the two management paths and exact provider request headers', async () => {
    expect(server.requests.every((r) => r.authorized && [AUTH_FILES, API_CALL].includes(r.path!))).toBe(true);
    expect(server.requests.every((r) => r.headers.authorization === `Bearer ${DEFAULT_KEY}`)).toBe(true);
    const posts = server.requests.filter((r) => r.path === API_CALL);
    expect(posts.length).toBeGreaterThan(0);
    for (const request of posts) {
      expect(request.method).toBe('POST');
      expect(request.headers['content-type']).toBe('application/json');
      if (request.body?.auth_index === 'codex-alice') {
        expect(request.body).toEqual({ auth_index: 'codex-alice', method: 'GET', url: CODEX_USAGE, header: {
          Authorization: 'Bearer $TOKEN$', 'Content-Type': 'application/json', 'OpenAI-Beta': 'codex-1',
          Originator: 'Codex Desktop', 'Chatgpt-Account-Id': 'fixture-account-alice',
        } });
      } else {
        expect(request.body).toEqual({ auth_index: 'claude-alice', method: 'GET', url: CLAUDE_USAGE, header: {
          Authorization: 'Bearer $TOKEN$', 'anthropic-beta': 'oauth-2025-04-20', 'User-Agent': 'claude-code/2.1.273',
        } });
      }
    }
    saveJson('management-requests.json', server.requests);
  });

  it('offers usage fields and distinct account keys in the widget editor', async () => {
    await $('[data-testid="nav-editor"]').click();
    await $('[data-testid="import-file"]').waitForExist();
    await browser.execute((content) => {
      const input = document.querySelector<HTMLInputElement>('[data-testid="import-file"]')!;
      const data = new DataTransfer();
      data.items.add(new File([content], 'accounts.json', { type: 'application/json' }));
      input.files = data.files;
      input.dispatchEvent(new Event('change', { bubbles: true }));
    }, readFileSync(accountLayout(), 'utf8'));
    await $('[data-testid="tile"][data-id="codex"]').waitForExist();
    await $('[data-testid="tile"][data-id="codex"]').click();
    await $('select[data-testid="action-account"]').waitForExist();
    const fields = await $$('[data-testid="action-field"] option').map((option) => option.getValue());
    expect(fields).toEqual(PROXY_FIELDS);
    const accounts = await $$('[data-testid="action-account"] option').map((option) => option.getValue());
    expect(accounts).toEqual(['', ...ACCOUNTS, 'codex:disabled@example.com']);
    await choose('action-account', 'claude:alice@example.com');
    await choose('action-field', 'seven_day_pct');
    await $('[data-testid="tile"][data-id="claude"]').click();
    await $('[data-testid="tile"][data-id="codex"]').click();
    await browser.waitUntil(async () =>
      (await $('[data-testid="action-account"]').getValue()) === 'claude:alice@example.com' &&
      (await $('[data-testid="action-field"]').getValue()) === 'seven_day_pct',
    { timeout: 5_000, timeoutMsg: 'the edited account and field did not survive reselecting the tile' });
    expect(await $('[data-testid="action-account"]').getValue()).toBe('claude:alice@example.com');
    expect(await $('[data-testid="action-field"]').getValue()).toBe('seven_day_pct');
    await choose('action-source', 'claude-status');
    expect(await $('[data-testid="action-field"]').getTagName()).toBe('input');
    expect(await $('[data-testid="action-account"]').getTagName()).toBe('input');
    await openSources();
  });

  it('marks the proxy tile stale after three failed polls and clears it when the server returns', async function () {
    this.timeout(90_000);
    await openSources();
    await type('proxy-interval', '2');
    await save();
    expect(await $('[data-testid="proxy-interval"]').getValue()).toBe('60');
    await browser.pause(500);
    const port = server.port;
    const stopped = Date.now();
    await server.stop();
    const trace: { fails: number; stale: boolean }[] = [];
    for (let attempt = 1; attempt <= 3; attempt++) {
      await afterRefreshFloor();
      lastRefresh = Date.now();
      await $('[data-testid="proxy-refresh"]').click();
      const state = await until(async () => {
        const { proxy } = await invoke<Status>('sources_status');
        return proxy.fails >= attempt && proxy;
      }, 5_000, `failed poll ${attempt}`);
      trace.push({ fails: state.fails, stale: state.stale });
      expect(state.stale).toBe(attempt === 3);
    }
    const stale = await until(async () => tile(await mockSets(), 'proxy').find((set) => set.stale && set.at_ms >= stopped), 5_000, 'stale widget');
    const dashboardStale = await until(async () => dashboards(await mockSets()).find((set) => set.at_ms >= stopped && set.rows?.every((row) => row.value === 'stale')), 5_000, 'stale dashboard');
    await $('[data-testid="proxy-state"][data-state="stale"]').waitForExist({ timeout: 5_000 });
    await cardShot('proxy-stale');

    server = await start({ port, key: DEFAULT_KEY });
    await afterRefreshFloor();
    const returned = Date.now();
    lastRefresh = returned;
    await $('[data-testid="proxy-refresh"]').click();
    const cleared = await until(async () => tile(await mockSets(), 'proxy').find((set) => !set.stale && set.at_ms > stale.at_ms), 5_000, 'recovery after refresh');
    saveJson('three-fails.json', { trace, dashboard_when_stale: dashboardStale, cleared_after_ms: cleared.at_ms - returned });
    expect(dashboardStale.rows?.every((row) => row.value === 'stale')).toBe(true);
    expect(cleared.at_ms - returned).toBeLessThan(3_500);
    await $('[data-testid="proxy-state"][data-state="ok"]').waitForExist({ timeout: 5_000 });
  });

  it('warns about an HTTP hub outside loopback without contacting it', async () => {
    await openSources();
    const view = await invoke<SourcesView>('sources_view');
    await invoke('sources_save', { config: { ...view.config, cliproxy: false, cliproxy_url: 'http://example.com' } });
    await $('[data-testid="proxy-http-warning"]').waitForExist({ timeout: 5_000 });
    expect(await $('[data-testid="proxy-http-warning"]').getText()).toContain('management key travels unencrypted');
    await invoke('sources_save', { config: view.config });
  });
});
