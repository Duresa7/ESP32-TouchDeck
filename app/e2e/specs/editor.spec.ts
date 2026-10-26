import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { connectFirstDeck } from '../deck.ts';
import { note, out, shot } from '../report.ts';
import { counts, validateSource, type GridPage, type Layout, type Tile } from '../../src/lib/editor/validate.ts';

const examples = join(import.meta.dirname, '..', '..', '..', 'schema', 'examples');
const devText = readFileSync(join(examples, 'dev.json'), 'utf8');
const dev = JSON.parse(devText) as Layout;
const first = dev.pages[0] as GridPage;

interface Record {
  received: { m: string }[];
  layouts: { landscape: Layout | null; portrait: Layout | null };
}

const invoke = <T>(command: string) =>
  browser.execute(async (name) => {
    const tauri = (window as unknown as { __TAURI__: { core: { invoke: (cmd: string) => Promise<unknown> } } }).__TAURI__;
    return tauri.core.invoke(name);
  }, command) as Promise<T>;

const status = () => $('[data-testid="editor-status"]').getText();
const tileIds = () => $$('[data-testid="grid"] [data-testid="tile"]').map(async (box) => String(await box.getAttribute('data-id')));
const tile = (id: string) => $(`[data-testid="tile"][data-id="${id}"]`);

async function cell(id: string) {
  const box = tile(id);
  return [Number(await box.getAttribute('data-row')), Number(await box.getAttribute('data-col'))];
}

async function spans(id: string) {
  const box = tile(id);
  return [Number(await box.getAttribute('data-row-span')), Number(await box.getAttribute('data-col-span'))];
}

async function waitStatus(match: RegExp, timeout = 5_000) {
  await browser.waitUntil(async () => match.test(await status()), { timeout, interval: 50, timeoutMsg: `status never matched ${match}: "${await status()}"` });
  return status();
}

async function importText(text: string, name: string) {
  await browser.execute(
    (content, file) => {
      const input = document.querySelector<HTMLInputElement>('[data-testid="import-file"]')!;
      const data = new DataTransfer();
      data.items.add(new File([content], file, { type: 'application/json' }));
      input.files = data.files;
      input.dispatchEvent(new Event('change', { bubbles: true }));
    },
    text,
    name,
  );
}

/** Drags a tile by pointer events: from its top-left cell to a cell, or by its corner to a cell. */
async function drag(id: string, row: number, col: number, mode: 'move' | 'resize') {
  await browser.execute(
    (tileId, toRow, toCol, how) => {
      const grid = document.querySelector<HTMLElement>('[data-testid="grid"]')!;
      const box = grid.querySelector<HTMLElement>(`[data-testid="tile"][data-id="${tileId}"]`)!;
      const rect = grid.getBoundingClientRect();
      const rows = Number(grid.dataset.rows);
      const cols = Number(grid.dataset.cols);
      const at = (r: number, c: number) => ({ clientX: rect.left + ((c + 0.5) * rect.width) / cols, clientY: rect.top + ((r + 0.5) * rect.height) / rows });
      const pointer = (type: string, point: { clientX: number; clientY: number }) =>
        new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: 1, pointerId: 1, isPrimary: true, pointerType: 'mouse', ...point });
      const target = how === 'resize' ? box.querySelector<HTMLElement>('[data-testid="resize-handle"]')! : box;
      const handle = target.getBoundingClientRect();
      const start = how === 'resize' ? { clientX: handle.left + 2, clientY: handle.top + 2 } : at(Number(box.dataset.row), Number(box.dataset.col));
      target.dispatchEvent(pointer('pointerdown', start));
      window.dispatchEvent(pointer('pointermove', at(toRow, toCol)));
      window.dispatchEvent(pointer('pointerup', at(toRow, toCol)));
    },
    id,
    row,
    col,
    mode,
  );
  await browser.pause(150);
}

/** Sets a form field the way typing or picking does, with the event the app listens for. */
async function setField(testid: string, value: string | boolean) {
  await browser.execute(
    (id, next) => {
      const field = document.querySelector<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>(`[data-testid="${id}"]`);
      if (!field) throw new Error(`no field ${id}`);
      if (field instanceof HTMLInputElement && field.type === 'checkbox') {
        field.checked = next === true;
        field.dispatchEvent(new Event('change', { bubbles: true }));
      } else {
        field.value = String(next);
        field.dispatchEvent(new Event(field instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true }));
      }
    },
    testid,
    value,
  );
  await browser.pause(60);
}

async function exportText(): Promise<string> {
  await browser.execute(() => {
    const host = window as unknown as { tdExported?: string; showSaveFilePicker?: unknown };
    host.tdExported = undefined;
    host.showSaveFilePicker = async () => ({
      createWritable: async () => ({
        write: async (data: string) => {
          host.tdExported = data;
        },
        close: async () => {},
      }),
    });
  });
  await $('[data-testid="export"]').click();
  await browser.waitUntil(async () => (await browser.execute(() => (window as unknown as { tdExported?: string }).tdExported)) !== undefined, {
    timeout: 5_000,
    timeoutMsg: 'Export wrote nothing',
  });
  return browser.execute(() => (window as unknown as { tdExported: string }).tdExported);
}

const sorted = (value: unknown): unknown =>
  Array.isArray(value)
    ? value.map(sorted)
    : value && typeof value === 'object'
      ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, sorted(v)]))
      : value;

function save(name: string, value: unknown) {
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, name), JSON.stringify(value, null, 2) + '\n');
}

describe('layout editor on the mock deck', () => {
  before(async () => {
    note('port', await connectFirstDeck());
    await $('[data-testid="nav-editor"]').click();
    await $('[data-testid="editor"]').waitForExist({ timeout: 5_000 });
  });

  it('imports dev.json with a tab per page and the first page drawn', async () => {
    await importText(devText, 'dev.json');
    await waitStatus(/^Imported /);
    const tabs = await $$('[data-testid="page-tab"]').map((tab) => tab.getAttribute('data-id'));
    expect(tabs).toEqual(dev.pages.map((page) => page.id));
    const drawn = await tileIds();
    expect([...drawn].sort()).toEqual(first.tiles.map((t) => t.id).sort());
    note('import', { tabs, page1_tiles: drawn.length, dev_page1_tiles: first.tiles.length, status: await status() });
    await shot('import');
  });

  it('moves f5 onto a free cell, then refuses a move onto quiet and keeps f5 in place', async () => {
    const free = [first.rows - 1, first.cols - 1];
    await drag('f5', free[0], free[1], 'move');
    expect(await cell('f5')).toEqual(free);
    await shot('drag-ok');
    const quiet = first.tiles.find((t) => t.id === 'quiet')!;
    await drag('f5', quiet.row, quiet.col, 'move');
    expect(await waitStatus(/overlaps/)).toBe('overlaps quiet');
    expect(await cell('f5')).toEqual(free);
    note('drag', { landed: free, refused: await status() });
    await shot('drag-refused');
  });

  it("pushes and shows the deck's page and tile counts", async () => {
    const { pages, tiles } = counts(dev);
    await $('[data-testid="push"]').click();
    const text = await waitStatus(/^Deck shows /);
    expect(text).toContain(`${pages} pages, ${tiles} tiles`);
    note('push', text);
    await shot('push');
  });

  it('grows spacer to 2 by 2, then refuses a size past the grid edge', async () => {
    const spacer = first.tiles.find((t) => t.id === 'spacer')!;
    await tile('spacer').click();
    await drag('spacer', spacer.row + 1, spacer.col + 1, 'resize');
    expect(await spans('spacer')).toEqual([2, 2]);
    await drag('spacer', spacer.row + 1, first.cols, 'resize');
    expect(await waitStatus(/outside/)).toBe(`outside the ${first.rows}x${first.cols} grid`);
    expect(await spans('spacer')).toEqual([2, 2]);
    note('resize', { grown: [2, 2], refused: await status() });
    await shot('resize');
  });

  it('sets a key, a launch, and a command action on new tiles and pushes them', async () => {
    await $('[data-testid="add-page"]').click();
    await browser.waitUntil(async () => (await $$('[data-testid="page-tab"]').length) === dev.pages.length + 1);
    for (let n = 0; n < 3; n++) await $('[data-testid="add-tile"]').click();
    const ids = await tileIds();
    expect(ids).toHaveLength(3);

    await tile(ids[0]).click();
    await setField('action-type', 'key');
    await setField('action-key', '62');
    await setField('action-mod-ctrl', true);
    await setField('action-mod-shift', true);
    await setField('tile-label', 'Chord F5');

    await tile(ids[1]).click();
    await setField('action-type', 'launch');
    await setField('action-path', 'C:\\Windows\\System32\\notepad.exe');
    await setField('tile-label', 'Notepad');

    await tile(ids[2]).click();
    await setField('action-type', 'command');
    await setField('action-program', 'cmd');
    await setField('action-args', '/c\necho\nhello');
    await setField('tile-label', 'Echo');
    await shot('actions');

    await $('[data-testid="push"]').click();
    await waitStatus(/^Deck shows /);
    const record = await invoke<Record>('e2e_mock_record');
    const pushed = record.layouts.landscape!;
    save('pushed.json', pushed);
    const added = (pushed.pages.at(-1) as GridPage).tiles;
    const action = (id: string) => added.find((t: Tile) => t.id === id)!.action;
    expect(action(ids[0])).toEqual({ type: 'key', usage: 62, mods: ['ctrl', 'shift'] });
    expect(action(ids[1])).toEqual({ type: 'launch', path: 'C:\\Windows\\System32\\notepad.exe', args: [], cwd: '' });
    expect(action(ids[2])).toEqual({ type: 'command', program: 'cmd', args: ['/c', 'echo', 'hello'], cwd: '', shell: false });
    note('actions', { tiles: ids, puts: record.received.filter((m) => m.m === 'layout.put').length });
  });

  it("pulls the deck's layout after a confirm and every tile matches", async () => {
    const record = await invoke<Record>('e2e_mock_record');
    await tile((await tileIds())[0]).click();
    await $('[data-testid="remove-tile"]').click();
    await $('[data-testid="pull"]').click();
    await $('[data-testid="pull-confirm"]').waitForExist({ timeout: 5_000 });
    await shot('pull-confirm');
    await $('[data-testid="pull-replace"]').click();
    await waitStatus(/^Pulled /);
    const pulled = JSON.parse(await exportText()) as Layout;
    expect(sorted(pulled)).toEqual(sorted(record.layouts.landscape));
    note('pull', { status: await status(), pages: pulled.pages.length, tiles: counts(pulled).tiles });
    await shot('pull');
  });

  it('exports a file that passes the layout rules', async () => {
    const text = await exportText();
    mkdirSync(out, { recursive: true });
    writeFileSync(join(out, 'export.json'), text);
    expect(validateSource(new TextEncoder().encode(text))).toEqual([]);
    note('export', { bytes: text.length, file: join(out, 'export.json') });
  });

  it('renders a 64-tile page and times five pushes', async () => {
    const tiles = Array.from({ length: 64 }, (_, i) => ({
      id: `t${i}`,
      label: `T${i}`,
      row: Math.floor(i / 8),
      col: i % 8,
      rowSpan: 1,
      colSpan: 1,
      icon: null,
      action: { type: 'key', usage: 58 + (i % 12), mods: [] },
    }));
    const big = { schema: 1, name: 'Sixty-four', orientation: 'landscape', pages: [{ id: 'all', title: 'All', kind: 'grid', rows: 8, cols: 8, tiles }] };
    await browser.execute(() => performance.clearMeasures('page-render'));
    await importText(JSON.stringify(big), 'sixty-four.json');
    await browser.waitUntil(async () => (await $$('[data-testid="grid"] [data-testid="tile"]').length) === 64, { timeout: 5_000 });
    await browser.pause(200);
    const render = await browser.execute(() => performance.getEntriesByName('page-render').map((entry) => entry.duration));
    await importText(devText, 'dev.json');
    await waitStatus(/^Imported /);
    await browser.execute(() => performance.clearMeasures('push'));
    for (let n = 0; n < 5; n++) {
      await $('[data-testid="push"]').click();
      await waitStatus(/^Deck shows /);
      await browser.pause(100);
    }
    const pushes = await browser.execute(() => performance.getEntriesByName('push').map((entry) => entry.duration));
    note('perf', { page_render_64_ms: render, push_dev_ms: pushes });
    expect(Math.max(...render)).toBeLessThan(200);
    expect(pushes).toHaveLength(5);
    expect(Math.max(...pushes)).toBeLessThan(500);
  });
});
