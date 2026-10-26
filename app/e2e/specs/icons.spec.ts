import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { connectFirstDeck } from '../deck.ts';
import { note, out, shot } from '../report.ts';

const index = JSON.parse(readFileSync(join(import.meta.dirname, '..', '..', 'icons', 'builtin', 'index.json'), 'utf8')) as { icons: { name: string }[] };
const NOTEPAD = 'C:\\Windows\\notepad.exe';

function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.execute(
    async (name, payload) => {
      const tauri = (window as unknown as { __TAURI__: { core: { invoke: (cmd: string, args: object) => Promise<unknown> } } }).__TAURI__;
      const value = await tauri.core.invoke(name, payload);
      // An icon file arrives as an ArrayBuffer, which WebDriver cannot return as is.
      return value instanceof ArrayBuffer ? Array.from(new Uint8Array(value)) : value;
    },
    command,
    args,
  ) as Promise<T>;
}

interface MockRecord {
  icons: string[];
  layouts: { landscape: unknown };
}

const status = () => $('[data-testid="editor-status"]').getText();
const record = () => invoke<MockRecord>('e2e_mock_record');
const glyph = (tile: string) => $(`[data-testid="tile"][data-id="${tile}"] [data-testid="tile-glyph"]`);

/** How many pixels a tile's glyph canvas draws with any alpha. */
const glyphOpaque = (tile: string) =>
  browser.execute((id) => {
    const canvas = document.querySelector<HTMLCanvasElement>(`[data-testid="tile"][data-id="${id}"] [data-testid="tile-glyph"]`);
    const data = canvas?.getContext('2d')?.getImageData(0, 0, canvas.width, canvas.height).data ?? new Uint8ClampedArray();
    return data.filter((a, i) => i % 4 === 3 && a > 0).length;
  }, tile);

async function waitStatus(match: RegExp, timeout = 10_000) {
  await browser.waitUntil(async () => match.test(await status()), { timeout, interval: 50, timeoutMsg: `status never matched ${match}: "${await status()}"` });
  return status();
}

async function setField(testid: string, value: string) {
  await browser.execute(
    (id, next) => {
      const field = document.querySelector<HTMLInputElement | HTMLSelectElement>(`[data-testid="${id}"]`);
      if (!field) throw new Error(`no field ${id}`);
      field.value = next;
      field.dispatchEvent(new Event(field instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true }));
    },
    testid,
    value,
  );
  await browser.pause(80);
}

async function addTile(): Promise<string> {
  const before = await $$('[data-testid="grid"] [data-testid="tile"]').map((box) => box.getAttribute('data-id'));
  await $('[data-testid="add-tile"]').click();
  await browser.waitUntil(async () => (await $$('[data-testid="grid"] [data-testid="tile"]').length) === before.length + 1);
  const after = await $$('[data-testid="grid"] [data-testid="tile"]').map((box) => box.getAttribute('data-id'));
  return String(after.find((id) => !before.includes(id)));
}

async function openPicker() {
  const toggle = $('[data-testid="icon-picker-toggle"]');
  if ((await toggle.getText()) === 'Choose icon') await toggle.click();
  await $('[data-testid="icon-grid"]').waitForExist({ timeout: 5_000 });
}

/** The editor's current layout, read back through its export. */
async function currentLayout(): Promise<string> {
  await browser.execute(() => {
    const host = window as unknown as { tdExported?: string; showSaveFilePicker?: unknown };
    host.tdExported = undefined;
    host.showSaveFilePicker = async () => ({ createWritable: async () => ({ write: async (data: string) => void (host.tdExported = data), close: async () => {} }) });
  });
  await $('[data-testid="export"]').click();
  await browser.waitUntil(async () => (await browser.execute(() => (window as unknown as { tdExported?: string }).tdExported)) !== undefined, { timeout: 5_000 });
  return JSON.stringify(JSON.parse(await browser.execute(() => (window as unknown as { tdExported: string }).tdExported)));
}

/** Each picker option's name and how many pixels its canvas draws with any alpha. */
function drawnOptions() {
  return [...document.querySelectorAll<HTMLElement>('[data-testid="icon-option"]')].map((option) => {
    const canvas = option.querySelector('canvas');
    const alpha = canvas?.getContext('2d')?.getImageData(0, 0, canvas.width, canvas.height).data.filter((_, i) => i % 4 === 3) ?? [];
    return { name: option.dataset.name ?? '', opaque: alpha.filter((a) => a > 0).length };
  });
}

function save(name: string, value: unknown) {
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, name), JSON.stringify(value, null, 2) + '\n');
}

describe('tile icons on the mock deck', () => {
  let terminalTile = '';
  let terminalId = '';

  before(async () => {
    note('port', await connectFirstDeck());
    await $('[data-testid="nav-editor"]').click();
    await $('[data-testid="editor"]').waitForExist({ timeout: 5_000 });
    await $('[data-testid="new-layout"]').click();
    await waitStatus(/^New layout/);
  });

  it('draws every built-in icon in the picker grid', async () => {
    terminalTile = await addTile();
    await openPicker();
    await browser.waitUntil(
      async () =>
        (await browser.execute(drawnOptions)).filter((option) => option.opaque > 0).length === index.icons.length,
      { timeout: 10_000, timeoutMsg: 'not every built-in icon drew' },
    ).catch(async (error) => {
      const seen = await browser.execute(drawnOptions);
      throw new Error(`${String(error)}; ${seen.length} options: ${seen.map((option) => `${option.name}:${option.opaque}`).join(' ')}`);
    });
    const names = await $$('[data-testid="icon-option"]').map((option) => option.getAttribute('data-name'));
    expect([...names].sort()).toEqual(index.icons.map((icon) => icon.name).sort());
    note('builtin', { index: index.icons.length, drawn: names.length });
    await shot('all-builtin');
  });

  it('searches term and picks terminal, and the tile shows its glyph', async () => {
    await setField('icon-search', 'term');
    const names = await $$('[data-testid="icon-option"]').map((option) => option.getAttribute('data-name'));
    expect(names).toContain('terminal');
    const option = $('[data-testid="icon-option"][data-name="terminal"]');
    terminalId = String(await option.getAttribute('data-id'));
    await option.click();
    await glyph(terminalTile).waitForExist({ timeout: 5_000 });
    expect(await glyph(terminalTile).getAttribute('data-id')).toBe(terminalId);
    const drawn = await glyphOpaque(terminalTile);
    expect(drawn).toBeGreaterThan(0);
    expect(await $('[data-testid="tile-icon"]').getValue()).toBe(`file:${terminalId}`);
    note('picker', { search: 'term', matches: names, icon: `file:${terminalId}`, tile_glyph_pixels: drawn });
    await shot('picker');
  });

  it('pushes the terminal icon once, and a second push sends none', async () => {
    await $('[data-testid="push"]').click();
    const text = await waitStatus(/^Deck shows .*1 icon sent$/);
    expect((await record()).icons).toEqual([terminalId]);
    const again = await invoke<{ state: { icons_loaded: number; icons_missing: number }; icons: { sent: string[] } }>('push_layout', { layout: await currentLayout() });
    expect(again.icons.sent).toEqual([]);
    expect(again.state.icons_loaded).toBe(1);
    expect(again.state.icons_missing).toBe(0);
    note('push', { status: text, second_sent: again.icons.sent.length, icons_loaded: again.state.icons_loaded, deck_icons: (await record()).icons });
    save('push-twice.json', { first: text, second: again });
    await shot('push');
  });

  it('takes Notepad\'s icon for a launch tile and pushes it as one icon', async function () {
    if (process.platform !== 'win32') {
      note('app_icon', 'skipped: the Notepad check runs on Windows only (G19)');
      this.skip();
    }
    const id = await addTile();
    await setField('action-type', 'launch');
    await setField('action-path', NOTEPAD);
    await $('[data-testid="use-app-icon"]').click();
    await glyph(id).waitForExist({ timeout: 10_000 });
    const iconId = String(await glyph(id).getAttribute('data-id'));
    const drawn = await glyphOpaque(id);
    expect(drawn).toBeGreaterThan(0);
    const file = await invoke<number[]>('icon_file', { id: iconId });
    const opaque = file.slice(12 + 8192).filter((a) => a === 255).length;
    expect(file.length).toBe(12300);
    expect(opaque).toBeGreaterThanOrEqual(500);
    mkdirSync(out, { recursive: true });
    writeFileSync(join(out, 'icon.bin'), Buffer.from(file));
    await $('[data-testid="push"]').click();
    const text = await waitStatus(/^Deck shows .*1 icon sent$/);
    expect((await record()).icons).toContain(iconId);
    note('app_icon', { path: NOTEPAD, id: iconId, opaque, tile_glyph_pixels: drawn, status: text });
    await shot('app-icon');
  });

  it('removes only icons no layout names', async () => {
    await $(`[data-testid="tile"][data-id="${terminalTile}"]`).click();
    await openPicker();
    await setField('icon-search', 'browser');
    const browserId = String(await $('[data-testid="icon-option"][data-name="browser"]').getAttribute('data-id'));
    await $('[data-testid="icon-option"][data-name="browser"]').click();
    await $('[data-testid="push"]').click();
    await waitStatus(/^Deck shows .*1 icon sent$/);
    const before = (await record()).icons;
    expect(before).toContain(terminalId);
    await $('[data-testid="remove-unused-icons"]').click();
    const text = await waitStatus(/unused icons? removed$/);
    const after = (await record()).icons;
    expect(text).toBe('1 unused icon removed');
    expect(after).not.toContain(terminalId);
    expect(after).toContain(browserId);
    expect(after.length).toBe(before.length - 1);
    note('prune', { before, after, status: text });
    save('prune.json', { before, after, removed: [terminalId], status: text });
    await shot('prune');
  });
});
