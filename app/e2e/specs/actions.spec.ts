import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectFirstDeck, openSettings } from '../deck.ts';
import { note, shot } from '../report.ts';

// The launch and command tiles run this Node, so the spec needs no per-OS program and opens no window.
const node = process.execPath;
const scratch = join(tmpdir(), `td-actions-${process.pid}-${Date.now()}`);
const launched = join(scratch, 'launched.txt');
const ran = join(scratch, 'ran.txt');

function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.execute(
    async (name, payload) => {
      const tauri = (window as unknown as { __TAURI__: { core: { invoke: (cmd: string, args: object) => Promise<unknown> } } }).__TAURI__;
      return tauri.core.invoke(name, payload);
    },
    command,
    args,
  ) as Promise<T>;
}

const press = (tile: string, kind: string) => invoke<void>('e2e_emit', { events: [{ m: 'press', page: 'desktop', tile, kind }] });

interface Tile {
  id: string;
  action: Record<string, unknown>;
}

function layoutWith(marker: string) {
  const layout = JSON.parse(readFileSync(join(import.meta.dirname, '..', '..', '..', 'schema', 'examples', 'dev.json'), 'utf8'));
  const desktop = layout.pages.find((page: { id: string }) => page.id === 'desktop');
  const tile = (id: string) => desktop.tiles.find((candidate: Tile) => candidate.id === id) as Tile;
  tile('launch').action = { type: 'launch', path: node, args: ['-e', `require('fs').writeFileSync(${JSON.stringify(launched)}, 'launched')`], cwd: '' };
  tile('command').action = {
    type: 'command',
    program: node,
    args: ['-e', `require('fs').appendFileSync('ran.txt', ${JSON.stringify(marker)} + String.fromCharCode(10))`],
    cwd: scratch,
    shell: false,
  };
  return layout;
}

async function load(marker: string) {
  const layout = layoutWith(marker);
  const summary = await invoke<{ name: string; tiles: number }>('set_action_layout', { json: JSON.stringify(layout), source: 'actions.spec' });
  expect(summary.name).toBe(layout.name);
  return layout.pages.find((page: { id: string }) => page.id === 'desktop').tiles.find((t: Tile) => t.id === 'command').action;
}

const runs = () => (existsSync(ran) ? readFileSync(ran, 'utf8').split('\n').filter(Boolean) : []);
const modal = () => $('[data-testid="approval-modal"]');

async function waitForRuns(count: number) {
  await browser.waitUntil(async () => runs().length >= count, { timeout: 5_000, interval: 50, timeoutMsg: `the command ran ${runs().length} times, not ${count}` });
  await browser.pause(200);
  expect(runs()).toHaveLength(count);
}

async function lastEntry(tile: string) {
  const entries = await $$(`[data-testid="action-entry"][data-tile="${tile}"]`).map(async (entry) => ({
    status: await entry.getAttribute('data-status'),
    exit: await entry.getAttribute('data-exit'),
    pid: await entry.getAttribute('data-pid'),
  }));
  return entries.at(-1);
}

describe('tile actions and command approval on the mock deck', () => {
  before(() => {
    rmSync(scratch, { recursive: true, force: true });
    mkdirSync(scratch, { recursive: true });
  });

  after(() => rmSync(scratch, { recursive: true, force: true }));

  it('connects and loads a layout for the actions', async () => {
    await connectFirstDeck();
    await openSettings('help');
    await load('first');
    await browser.waitUntil(async () => (await $('[data-testid="action-layout"]').getText()).includes('actions.spec'), { timeout: 3_000 });
  });

  it('launches a program on a press without asking', async () => {
    const started = Date.now();
    await press('launch', 'launch');
    await browser.waitUntil(() => existsSync(launched), { timeout: 2_000, interval: 25, timeoutMsg: 'the launched program left no marker within 2 s' });
    note('launch_ms', Date.now() - started);
    expect(await modal().isExisting()).toBe(false);
    await browser.waitUntil(async () => (await lastEntry('launch'))?.status === 'started', { timeout: 2_000 });
    expect(Number((await lastEntry('launch'))?.pid)).toBeGreaterThan(0);
  });

  it('shows the exact command, runs it on Allow once, and asks again on the next press', async () => {
    const action = (await load('once')) as { program: string; args: string[]; cwd: string };
    await press('command', 'command');
    await modal().waitForDisplayed({ timeout: 3_000 });
    expect(await $('[data-testid="approval-program"]').getText()).toBe(action.program);
    expect(await $$('[data-testid="approval-arg"]').map((arg) => arg.getText())).toEqual(action.args);
    expect(await $('[data-testid="approval-cwd"]').getText()).toBe(action.cwd);
    expect(await $('[data-testid="approval-shell"]').getText()).toBe('No');
    expect(await $('[data-testid="approval-shell"]').getAttribute('data-shell')).toBe('false');
    note('approval', { program: action.program, args: action.args, cwd: action.cwd, hash: await modal().getAttribute('data-hash') });
    await shot('approve');
    expect(runs()).toHaveLength(0);
    await $('[data-testid="approval-once"]').click();
    await waitForRuns(1);
    await browser.waitUntil(async () => (await lastEntry('command'))?.exit === '0', { timeout: 3_000, timeoutMsg: 'no exit code 0 in the action log' });

    await press('command', 'command');
    await modal().waitForDisplayed({ timeout: 3_000 });
    await $('[data-testid="approval-deny"]').click();
    await modal().waitForExist({ reverse: true, timeout: 2_000 });
    await browser.waitUntil(async () => (await lastEntry('command'))?.status === 'denied', { timeout: 2_000 });
    expect(runs()).toHaveLength(1);
  });

  it('never asks again after Always allow, and asks for a changed argument', async () => {
    await load('always');
    await press('command', 'command');
    await modal().waitForDisplayed({ timeout: 3_000 });
    await $('[data-testid="approval-always"]').click();
    await waitForRuns(2);

    await press('command', 'command');
    await waitForRuns(3);
    expect(await modal().isExisting()).toBe(false);
    expect(runs()).toEqual(['once', 'always', 'always']);

    await load('changed');
    await press('command', 'command');
    await modal().waitForDisplayed({ timeout: 3_000 });
    expect((await $$('[data-testid="approval-arg"]').map((arg) => arg.getText()))[1]).toContain('changed');
    await shot('always-changed');
    await $('[data-testid="approval-deny"]').click();
    await browser.pause(500);
    expect(runs()).toEqual(['once', 'always', 'always']);

    await load('always');
    await press('command', 'command');
    await waitForRuns(4);
    expect(await modal().isExisting()).toBe(false);
  });

  it('logs a press on a tile the layout does not hold', async () => {
    await invoke('e2e_emit', { events: [{ m: 'press', page: 'desktop', tile: 'nope', kind: 'command' }] });
    await browser.waitUntil(async () => (await lastEntry('nope'))?.status === 'unknown', { timeout: 5_000 });
  });
});
