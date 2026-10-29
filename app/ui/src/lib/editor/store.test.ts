import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { createClient, type Companion } from '../../companion/client.ts';
import { createDevicesStore } from '../../companion/devices.ts';
import { ApiError } from '../../companion/errors.ts';
import { openEvents, type Events } from '../../companion/events.ts';
import { createFakeCompanion, type FakeCompanion } from '../../companion/fake.ts';
import { createJobsStore, waitForJob, type JobsStore } from '../../companion/jobs.ts';
import { sampleDevice } from '../../companion/samples.ts';
import { windowSession } from '../../companion/session.ts';
import { draftKey, type DraftStorage } from './drafts.ts';
import { createEditorStore, saveState, type EditorDeps, type EditorStore } from './store.ts';
import { canonical, starterLayout, type GridPage, type Layout, type Page } from './validate.ts';

const BASE = 'http://127.0.0.1:51234';
const DESK = { id: 'mock0001', name: 'Desk' };
const connected = { state: 'connected' as const, port: 'COM3', since_ms: 0, protocol: 2 };

function memoryStorage(): DraftStorage & { items: Map<string, string> } {
  const items = new Map<string, string>();
  return {
    items,
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: (key) => void items.delete(key),
  };
}

let fake: FakeCompanion;
let companion: Companion;
let jobs: JobsStore;
let storage: ReturnType<typeof memoryStorage>;

function editor(overrides: Partial<EditorDeps> = {}): EditorStore {
  return createEditorStore({ companion, storage, wait: (job) => waitForJob(jobs, companion, job, 10), ...overrides });
}

const state = (store: EditorStore) => store.getState();
const draft = (store: EditorStore) => state(store).drafts[state(store).orientation]!.layout;
function gridPage(page: Page | undefined): GridPage {
  if (page?.kind !== 'grid') throw new Error(`${page?.id} is not a grid page`);
  return page;
}
const page = (store: EditorStore) => gridPage(draft(store).pages.find((p) => p.id === state(store).page));
const tile = (store: EditorStore, id: string) => page(store).tiles.find((t) => t.id === id)!;

beforeEach(() => {
  fake = createFakeCompanion({
    devices: [sampleDevice('mock0001', 'Desk', connected), sampleDevice('mock0002', 'Studio', connected)],
    layouts: { mock0001: starterLayout('landscape') },
    jobMs: 5,
  });
  vi.stubGlobal('fetch', fake.fetch);
  companion = createClient(BASE, windowSession(fake.bridge));
  jobs = createJobsStore();
  storage = memoryStorage();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('editing a draft', () => {
  let store: EditorStore;

  beforeEach(() => {
    store = editor();
    store.open(DESK);
    store.startStarter();
  });

  test('a Starter Layout opens on its first Page with nothing selected', () => {
    expect(draft(store).name).toBe('Starter');
    expect(state(store).page).toBe('media');
    expect(state(store).selected).toBeNull();
    expect(state(store).status?.text).toBe('Started the Starter Layout.');
  });

  test('a Tile moves onto a free cell, and a move onto a taken cell is refused with the Tile kept', () => {
    store.setGrid(3, 3);
    expect(store.moveTile('prev', 2, 2)).toBeNull();
    expect(tile(store, 'prev')).toMatchObject({ row: 2, col: 2 });

    expect(store.moveTile('prev', 0, 1)).toBe('overlaps playpause');
    expect(state(store).status).toMatchObject({ kind: 'error', text: 'That spot is taken by “Play/Pause”. Pick an empty one.', event: 'refused' });
    expect(tile(store, 'prev')).toMatchObject({ row: 2, col: 2 });
  });

  test('a Tile grows to 2 × 2, and a size past the grid edge is refused', () => {
    store.setGrid(3, 3);
    store.clearTile('playpause');
    store.clearTile('mute');
    store.clearTile('voldown');
    expect(store.resizeTile('prev', 2, 2)).toBeNull();
    expect(tile(store, 'prev')).toMatchObject({ rowSpan: 2, colSpan: 2 });

    expect(store.resizeTile('prev', 2, 4)).toBe('outside the 3x3 grid');
    expect(state(store).status?.text).toBe("That doesn't fit. This Page has 3 rows and 3 columns.");
    expect(tile(store, 'prev')).toMatchObject({ rowSpan: 2, colSpan: 2 });
  });

  test('Add Tile targets a clicked cell, places the kind’s default action there and selects it', () => {
    store.setGrid(3, 3);
    store.openAdd({ row: 2, col: 1 });
    expect(state(store).adding).toEqual({ row: 2, col: 1 });
    store.place('consumer');
    const added = state(store).selected!;
    expect(tile(store, added)).toMatchObject({ row: 2, col: 1, label: 'Tile', action: { type: 'consumer', usage: 205 } });
    expect(state(store).adding).toBeNull();
  });

  test('Add Tile with no cell picks the first free one', () => {
    store.setGrid(3, 3);
    store.openAdd();
    expect(state(store).adding).toEqual({ row: 2, col: 0 });
  });

  test('a kind that needs a value first starts as Nothing and remembers the kind', () => {
    store.setGrid(3, 3);
    store.openAdd();
    store.place('launch');
    const added = state(store).selected!;
    expect(tile(store, added).action).toEqual({ type: 'none' });
    expect(state(store).intents[added]).toBe('launch');
  });

  test('Add Tile on a full Page starts a new Page with the Tile on it', () => {
    store.openAdd();
    expect(state(store).adding).toBe('new-page');
    store.place('key');
    expect(draft(store).pages).toHaveLength(3);
    expect(state(store).page).toBe('page1');
    expect(page(store).tiles).toEqual([expect.objectContaining({ row: 0, col: 0, action: { type: 'key', usage: 58, mods: [] } })]);
  });

  test('Duplicate copies a Tile into the next free cell and selects the copy', () => {
    store.setGrid(3, 3);
    store.select('next');
    store.duplicate('next');
    const copy = state(store).selected!;
    expect(copy).not.toBe('next');
    expect(tile(store, copy)).toMatchObject({ row: 2, col: 0, label: 'Next', action: { type: 'consumer', usage: 181 } });
  });

  test('Duplicate on a full Page is refused', () => {
    store.duplicate('next');
    expect(state(store).status?.text).toBe('This Page is full. Make its grid bigger or add a Page.');
    expect(page(store).tiles).toHaveLength(6);
  });

  test('Clear Tile removes it and drops the selection', () => {
    store.select('mute');
    store.clearTile('mute');
    expect(page(store).tiles.map((t) => t.id)).not.toContain('mute');
    expect(state(store).selected).toBeNull();
  });

  test('Pages: add, rename, refuse a blank name and a grid that cuts a Tile, remove', () => {
    store.addPage();
    expect(state(store).page).toBe('page1');
    expect(page(store).title).toBe('Page 3');

    expect(store.renamePage('page1', 'Games')).toBeNull();
    expect(page(store).title).toBe('Games');
    expect(store.renamePage('page1', '')).not.toBeNull();
    expect(state(store).status?.text).toBe('Give the Page a name.');

    store.selectPage('media');
    expect(store.setGrid(1, 3)).toBe('grid.too-small: mute');
    expect(state(store).status?.text).toBe('The grid can\'t shrink past “Mute”. Move or remove it first.');

    store.removePage('page1');
    expect(draft(store).pages.map((p) => p.id)).toEqual(['media', 'shortcuts']);
  });

  test('the last Page cannot be removed', () => {
    store.startBlank();
    store.removePage('page1');
    expect(state(store).status?.text).toBe('A Layout needs at least one Page.');
  });

  test('a file that isn’t a Layout is explained and changes nothing', () => {
    const before = draft(store);
    expect(store.importText('{"schema": 1, "name": "Broken"', 'broken.json')).toBe(false);
    expect(state(store).status).toMatchObject({ kind: 'error', text: '“broken.json” isn\'t a Layout file. Nothing changed.', event: 'open-failed' });
    expect(draft(store)).toBe(before);
  });

  test('an imported file opens its own orientation with a tab per Page', () => {
    const portrait: Layout = { ...starterLayout('portrait'), name: 'Tall' };
    expect(store.importText(JSON.stringify(portrait), 'tall.json')).toBe(true);
    expect(state(store).orientation).toBe('portrait');
    expect(state(store).status?.text).toBe('Imported “Tall”: 2 Pages, 16 Tiles.');
    expect(state(store).drafts.landscape?.layout.name).toBe('Starter');
  });
});

describe('drafts', () => {
  test('each orientation keeps its own draft', () => {
    const store = editor();
    store.open(DESK);
    store.startStarter();
    store.setOrientation('portrait');
    expect(state(store).drafts.portrait).toBeNull();
    store.startBlank();
    expect(draft(store).orientation).toBe('portrait');
    store.setOrientation('landscape');
    expect(draft(store).name).toBe('Starter');
  });

  test('drafts are saved per Device and orientation and come back when the Device opens again', () => {
    const first = editor();
    first.open(DESK);
    first.startStarter();
    first.renameLayout('Mine');
    first.open({ id: 'mock0002', name: 'Studio' });
    expect(state(first).drafts.landscape).toBeNull();
    first.startBlank();

    expect([...storage.items.keys()].sort()).toEqual([draftKey('mock0001', 'landscape'), draftKey('mock0002', 'landscape')]);

    const second = editor();
    second.open(DESK);
    expect(draft(second).name).toBe('Mine');
    second.open({ id: 'mock0002', name: 'Studio' });
    expect(draft(second).name).toBe('New Layout');
  });

  test('a saved draft that no longer passes the rules is dropped', () => {
    storage.setItem(draftKey('mock0001', 'landscape'), JSON.stringify({ v: 1, layout: '{"schema":1}', sent: null, revision: null }));
    const store = editor();
    store.open(DESK);
    expect(state(store).drafts.landscape).toBeNull();
  });
});

describe('send and load', () => {
  test('Send shows Sending…, then the result with the Device’s counts, and the draft reads as saved', async () => {
    const store = editor();
    store.open(DESK);
    store.startStarter();
    expect(saveState(state(store))).toBe('unsent');
    const sending = store.push();
    expect(state(store).status).toMatchObject({ text: 'Sending…', event: 'pushing' });
    expect(saveState(state(store))).toBe('sending');
    await sending;
    expect(state(store).status).toMatchObject({ kind: 'ok', text: 'Sent to Desk', event: 'pushed', pages: 2, tiles: 16 });
    expect(state(store).status?.detail).toMatch(/^Desk shows Starter: 2 Pages, 16 Tiles \(\d+ bytes in \d+ ms\), 0 icons sent$/);
    expect(saveState(state(store))).toBe('saved');
    expect(fake.layouts.stored('mock0001', 'landscape')).toBe(canonical(draft(store)));

    store.renameLayout('Changed');
    expect(saveState(state(store))).toBe('unsent');
  });

  test('a Job follows job.updated from the event stream', async () => {
    const devices = createDevicesStore();
    const events: Events = openEvents({ url: 'ws://127.0.0.1:51234/api/v1/events', session: windowSession(fake.bridge), store: devices, jobs, open: fake.openSocket });
    await vi.waitFor(() => expect(devices.getState().status).toBe('live'));
    const store = editor({ wait: (job) => waitForJob(jobs, companion, job, 60_000) });
    store.open(DESK);
    store.startStarter();
    await store.push();
    expect(state(store).status?.event).toBe('pushed');
    expect(Object.values(jobs.getState().jobs).map((j) => j.state)).toEqual(['succeeded']);
    events.close();
  });

  test('Load from the Device with no draft loads at once', async () => {
    const store = editor();
    store.open(DESK);
    await store.load();
    expect(state(store).status).toMatchObject({ kind: 'ok', text: 'Loaded “Starter” from Desk', event: 'pulled' });
    expect(saveState(state(store))).toBe('saved');
  });

  test('Load from the Device asks before it replaces a draft that differs', async () => {
    const store = editor();
    store.open(DESK);
    store.startBlank();
    await store.load();
    expect(state(store).conflict).toMatchObject({ reason: 'load', layout: { name: 'Starter' } });
    expect(state(store).status?.text).toBe('Desk has “Starter”: 2 Pages, 16 Tiles.');
    expect(draft(store).name).toBe('New Layout');

    store.replace();
    expect(draft(store).name).toBe('Starter');
    expect(state(store).conflict).toBeNull();
    expect(canonical(draft(store))).toBe(fake.layouts.stored('mock0001', 'landscape'));
  });

  test('Keep mine keeps the draft, and the next send replaces what the Device has', async () => {
    const store = editor();
    store.open(DESK);
    store.startBlank();
    await store.load();
    store.keepMine();
    expect(state(store).status?.text).toBe('Kept your Layout');
    expect(draft(store).name).toBe('New Layout');
    await store.push();
    expect(state(store).status?.event).toBe('pushed');
    expect(JSON.parse(fake.layouts.stored('mock0001', 'landscape')!).name).toBe('New Layout');
  });

  test('a Device with no Layout for the orientation says so', async () => {
    const store = editor();
    store.open(DESK);
    store.setOrientation('portrait');
    await store.load();
    expect(state(store).status).toMatchObject({ kind: 'error', text: 'Desk has no portrait Layout yet.', event: 'pull-failed' });
  });

  test('412: a Layout changed on the Device opens Replace or Keep mine', async () => {
    const store = editor();
    store.open(DESK);
    await store.load();
    store.renameLayout('Mine');
    fake.layouts.store('mock0001', { ...starterLayout('landscape'), name: 'Elsewhere' });

    await store.push();
    expect(state(store).conflict).toMatchObject({ reason: 'changed', layout: { name: 'Elsewhere' } });
    expect(state(store).status).toMatchObject({ text: 'Desk has “Elsewhere”: 2 Pages, 16 Tiles.', event: 'changed' });
    expect(JSON.parse(fake.layouts.stored('mock0001', 'landscape')!).name).toBe('Elsewhere');

    store.keepMine();
    await store.push();
    expect(state(store).status?.event).toBe('pushed');
    expect(JSON.parse(fake.layouts.stored('mock0001', 'landscape')!).name).toBe('Mine');
  });

  test('412 then Replace makes the Device’s Layout the draft', async () => {
    const store = editor();
    store.open(DESK);
    await store.load();
    store.renameLayout('Mine');
    fake.layouts.store('mock0001', { ...starterLayout('landscape'), name: 'Elsewhere' });
    await store.push();
    store.replace();
    expect(draft(store).name).toBe('Elsewhere');
    expect(saveState(state(store))).toBe('saved');
  });

  test('422: the Companion’s issues become the rule message', async () => {
    const invalid = { code: 'layout.invalid', message: 'The Layout has 1 problems.', details: { issues: [{ rule: 'label.too-long', path: '/pages/0/tiles/0/label', message: '/pages/0/tiles/0/label must NOT have more than 16 characters (maxLength)' }] } };
    const store = editor({
      companion: { ...companion, layouts: { ...companion.layouts, push: () => Promise.reject(new ApiError(invalid.code, invalid.message, invalid.details)) } },
    });
    store.open(DESK);
    store.startStarter();
    await store.push();
    expect(state(store).status).toMatchObject({ kind: 'error', text: 'Tile labels can be up to 16 characters.', event: 'push-failed' });
    expect(state(store).status?.detail).toContain('label.too-long');
    expect(state(store).busy).toBeNull();
  });

  test('a busy Device says so', async () => {
    fake.layouts.hold('mock0001');
    const first = editor();
    first.open(DESK);
    first.startStarter();
    const holding = first.push();
    await vi.waitFor(() => expect(Object.keys(fake.layouts.jobs())).toHaveLength(1));

    const second = editor();
    second.open(DESK);
    second.startBlank();
    await second.push();
    expect(state(second).status).toMatchObject({ kind: 'error', text: 'Desk is busy. Wait a moment, then retry.' });

    fake.layouts.release('mock0001');
    await holding;
    expect(state(first).status?.event).toBe('pushed');
  });

  test('a Device that isn’t connected can’t take a send', async () => {
    fake.update('mock0001', { link: { state: 'lost', reason: 'unplugged', detail: '', at_ms: 0 } });
    const store = editor();
    store.open(DESK);
    store.startStarter();
    await store.push();
    expect(state(store).status?.text).toBe("Couldn't send to Desk. Check that it's plugged in, then retry.");
  });

  test('Remove unused Assets keeps the Assets the drafts use', async () => {
    const prune = vi.fn(companion.assets.prune);
    const store = editor({ companion: { ...companion, assets: { ...companion.assets, prune } } });
    store.open(DESK);
    const layout = starterLayout('landscape');
    gridPage(layout.pages[0]).tiles[0].icon = 'file:0123456789abcdef';
    store.importText(JSON.stringify(layout), 'icons.json');
    await store.pruneAssets();
    expect(prune).toHaveBeenCalledWith({ device: 'mock0001' }, null, { also_keep: ['0123456789abcdef'] });
    expect(state(store).status).toMatchObject({ kind: 'ok', text: 'Removed 0 unused Assets from Desk' });
  });
});
