import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { checkLayouts, validateLayout } from './check-layouts.mjs';

const directory = dirname(fileURLToPath(import.meta.url));
const read = (path) => JSON.parse(readFileSync(join(directory, path), 'utf8'));
const builtin = read('examples/builtin.json');
const dev = read('examples/dev.json');
const schema = read('layout.schema.json');
const copy = () => structuredClone(dev);

for (const name of ['builtin', 'dev']) {
  test(`${name} is valid without mutating the layout`, () => {
    const layout = read(`examples/${name}.json`);
    const before = structuredClone(layout);
    assert.deepEqual(validateLayout(layout), []);
    assert.deepEqual(layout, before);
  });
}

for (const filename of readdirSync(join(directory, 'examples/invalid')).filter((name) => name.endsWith('.json'))) {
  test(`rejects invalid example ${filename} with its first-line message`, () => {
    const source = readFileSync(join(directory, 'examples/invalid', filename), 'utf8');
    const layout = JSON.parse(source);
    const firstLine = JSON.parse(source.split(/\r?\n/, 1)[0].replace(/,$/, '}'));
    const expected = firstLine.name.slice('expected: '.length);
    assert.ok(expected.length > 0);
    assert.ok(validateLayout(layout).some((error) => error.includes(expected)));
  });
}

test('builtin preserves all milestone 1 pages, positions, actions, and icons', () => {
  assert.deepEqual(builtin.pages.map(({ id, title, kind, rows, cols }) => ({ id, title, kind, rows, cols })), [
    { id: 'fkeys', title: 'F-Keys', kind: 'grid', rows: 4, cols: 4 },
    { id: 'media', title: 'Media', kind: 'grid', rows: 2, cols: 3 },
  ]);
  const tiles = builtin.pages.flatMap((page) => page.tiles);
  assert.equal(tiles.length, 22);
  for (let index = 0; index < 12; index++) {
    const tile = tiles[index];
    assert.equal(tile.id, `f${index + 1}`);
    assert.equal(tile.label, `F${index + 1}`);
    assert.deepEqual([tile.row, tile.col, tile.rowSpan, tile.colSpan], [Math.floor(index / 4), index % 4, 1, 1]);
    assert.equal(tile.icon, null);
    assert.deepEqual(tile.action, { type: 'key', usage: 58 + index, mods: [] });
  }
  for (const [index, mod] of ['shift', 'ctrl', 'alt', 'win'].entries()) {
    assert.equal(tiles[12 + index].id, mod);
    assert.deepEqual(tiles[12 + index].action, { type: 'mod', mod });
  }
  assert.deepEqual(builtin.pages[1].tiles.map(({ id, label, icon, action }) => [id, label, icon, action.usage]), [
    ['prev', 'Prev', 'lv:prev', 182],
    ['playpause', 'Play/Pause', 'lv:play_pause', 205],
    ['next', 'Next', 'lv:next', 181],
    ['mute', 'Mute', 'lv:mute', 226],
    ['voldown', 'Vol -', 'lv:volume_mid', 234],
    ['volup', 'Vol +', 'lv:volume_max', 233],
  ]);
});

test('dev exercises every action, both spanning directions, and all requested widgets', () => {
  assert.deepEqual(dev.pages.map((page) => page.kind), ['grid', 'grid', 'grid', 'dashboard']);
  const tiles = dev.pages.flatMap((page) => page.tiles ?? []);
  const actions = tiles.map((tile) => tile.action);
  assert.deepEqual(new Set(actions.map((action) => action.type)), new Set(schema.$defs.action.oneOf.map((action) => action.properties.type.const)));
  assert.equal(tiles.filter((tile) => tile.rowSpan > 1 || tile.colSpan > 1).length, 2);
  assert.ok(tiles.some((tile) => tile.rowSpan > 1));
  assert.ok(tiles.some((tile) => tile.colSpan > 1));
  assert.ok(actions.some((action) => action.type === 'key' && action.mods.length > 0));
  assert.ok(actions.some((action) => action.type === 'page' && action.page === dev.pages[3].id));
  assert.equal(actions.filter((action) => action.type === 'widget' && action.source === 'claude-status').length, 3);
  assert.equal(actions.filter((action) => action.type === 'widget' && action.source === 'cliproxyapi').length, 1);
  assert.deepEqual(actions.find((action) => action.type === 'command'), { type: 'command', program: 'cmd', args: ['/c', 'echo', 'TouchDeck'], cwd: '', shell: false });
});

test('dev preserves the ids and roles used by later milestone lanes', () => {
  const tiles = new Map(dev.pages.flatMap((page) => (page.tiles ?? []).map((tile) => [tile.id, { page: page.id, action: tile.action }])));
  for (const id of ['f5', 'shift', 'chord', 'launch', 'command', 'open', 'context', 'cost', 'model', 'proxy']) {
    assert.ok(tiles.has(id), `missing fixture tile ${id}`);
  }
  assert.deepEqual(tiles.get('f5').action, { type: 'key', usage: 62, mods: [] });
  assert.deepEqual(tiles.get('shift').action, { type: 'mod', mod: 'shift' });
  assert.equal(tiles.get('f5').page, tiles.get('shift').page);
  assert.equal(tiles.get('chord').action.type, 'key');
  assert.deepEqual(new Set(tiles.get('chord').action.mods), new Set(['ctrl', 'shift']));
  assert.equal(tiles.get('launch').action.type, 'launch');
  assert.equal(tiles.get('open').action.type, 'open');
  assert.deepEqual(tiles.get('command').action, { type: 'command', program: 'cmd', args: ['/c', 'echo', 'TouchDeck'], cwd: '', shell: false });
  for (const [id, source, field] of [
    ['context', 'claude-status', 'context_pct'],
    ['cost', 'claude-status', 'cost_usd'],
    ['model', 'claude-status', 'model'],
    ['proxy', 'cliproxyapi', 'status'],
  ]) {
    const action = tiles.get(id).action;
    assert.deepEqual([action.type, action.source, action.field], ['widget', source, field]);
  }
  const accounts = dev.pages.find((page) => page.id === 'accounts');
  assert.deepEqual(accounts, { id: 'accounts', title: 'Accounts', kind: 'dashboard', source: 'cliproxyapi' });
  assert.ok([...tiles.values()].some(({ action }) => action.type === 'page' && action.page === 'accounts'));
});

test('overlap names both tiles once even when multiple cells overlap', () => {
  const layout = copy();
  const [first, second] = layout.pages[0].tiles;
  Object.assign(second, { row: first.row, col: first.col, colSpan: first.colSpan });
  assert.deepEqual(validateLayout(layout), ['page "shortcuts" tiles "chord" and "shift" overlap']);
});

test('overlap reports every pair when three tiles share a cell', () => {
  const layout = copy();
  const [first, second, third] = layout.pages[0].tiles;
  Object.assign(second, { row: first.row, col: first.col });
  Object.assign(third, { row: first.row, col: first.col });
  assert.deepEqual(validateLayout(layout), [
    'page "shortcuts" tiles "chord" and "shift" overlap',
    'page "shortcuts" tiles "chord" and "quiet" overlap',
    'page "shortcuts" tiles "shift" and "quiet" overlap',
  ]);
});

test('duplicate tile ids across pages report both locations', () => {
  const layout = copy();
  layout.pages[1].tiles[0].id = 'chord';
  assert.deepEqual(validateLayout(layout), ['duplicate id "chord" at page "desktop" tile "chord"; first used at page "shortcuts" tile "chord"']);
});

test('page ids share the tile id namespace', () => {
  const layout = copy();
  layout.pages[0].tiles[1].id = 'accounts';
  assert.deepEqual(validateLayout(layout), ['duplicate id "accounts" at page "accounts"; first used at page "shortcuts" tile "accounts"']);
});

test('duplicate page ids are rejected', () => {
  const layout = copy();
  layout.pages[1].id = layout.pages[0].id;
  assert.ok(validateLayout(layout).some((error) => error.startsWith('duplicate id "shortcuts"')));
});

test('missing page targets report page and tile', () => {
  const layout = copy();
  layout.pages[0].tiles[3].action.page = 'missing';
  assert.deepEqual(validateLayout(layout), ['page "shortcuts" tile "accounts_jump" references missing page "missing"']);
});

test('64 tiles fit and a 65th tile fails the tile count', () => {
  const layout = copy();
  layout.pages = [{ id: 'full', title: 'Full', kind: 'grid', rows: 8, cols: 8, tiles: Array.from({ length: 64 }, (_, index) => ({ id: `t_${index}`, label: 'Tile', row: Math.floor(index / 8), col: index % 8, rowSpan: 1, colSpan: 1, icon: null, action: { type: 'none' } })) }];
  assert.deepEqual(validateLayout(layout), []);
  layout.pages[0].tiles.push({ ...layout.pages[0].tiles[0], id: 't_64' });
  assert.ok(validateLayout(layout).some((error) => error.includes('must NOT have more than 64 items (maxItems)')));
});

for (const [name, mutate, message] of [
  ['unsupported version', (x) => { x.schema = 2; }, '(const)'],
  ['unknown orientation', (x) => { x.orientation = 'upside_down'; }, '(enum)'],
  ['no pages', (x) => { x.pages = []; }, '(minItems)'],
  ['nine pages', (x) => { x.pages = Array.from({ length: 9 }, (_, n) => ({ id: `p_${n}`, title: 'Page', kind: 'grid', rows: 1, cols: 1, tiles: [] })); }, '(maxItems)'],
  ['invalid id', (x) => { x.pages[0].id = 'Upper-Case'; }, '(pattern)'],
  ['16-character id', (x) => { x.pages[0].tiles[0].id = 'a'.repeat(16); }, '(pattern)'],
  ['empty grid title', (x) => { x.pages[0].title = ''; }, '(minLength)'],
  ['empty dashboard title', (x) => { x.pages[3].title = ''; }, '(minLength)'],
  ['empty tile label', (x) => { x.pages[0].tiles[0].label = ''; }, '(minLength)'],
  ['17-character label', (x) => { x.pages[0].tiles[0].label = 'a'.repeat(17); }, '(maxLength)'],
  ['17-code-point label', (x) => { x.pages[0].tiles[0].label = '😀'.repeat(17); }, '(maxLength)'],
  ['25-character title', (x) => { x.pages[0].title = 'a'.repeat(25); }, '(maxLength)'],
  ['25-code-point title', (x) => { x.pages[0].title = '😀'.repeat(25); }, '(maxLength)'],
  ['zero rows', (x) => { x.pages[0].rows = 0; }, '(minimum)'],
  ['nine cols', (x) => { x.pages[0].cols = 9; }, '(maximum)'],
  ['fractional coordinate', (x) => { x.pages[0].tiles[0].row = 0.5; }, '(type)'],
  ['negative coordinate', (x) => { x.pages[0].tiles[0].col = -1; }, '(minimum)'],
  ['zero span', (x) => { x.pages[0].tiles[0].rowSpan = 0; }, '(minimum)'],
  ['row outside page', (x) => { x.pages[0].tiles[0].row = 3; }, 'exceeds grid bounds'],
  ['column span outside page', (x) => { x.pages[0].tiles[0].colSpan = 5; }, 'exceeds grid bounds'],
  ['key usage zero', (x) => { x.pages[0].tiles[0].action.usage = 0; }, 'must be >= 1 (minimum)'],
  ['key usage 256', (x) => { x.pages[0].tiles[0].action.usage = 256; }, 'must be <= 255 (maximum)'],
  ['consumer usage zero', (x) => { x.pages[0].tiles[2].action.usage = 0; }, 'must be >= 1 (minimum)'],
  ['consumer usage 65536', (x) => { x.pages[0].tiles[2].action.usage = 65536; }, 'must be <= 65535 (maximum)'],
  ['repeated modifier', (x) => { x.pages[0].tiles[0].action.mods = ['ctrl', 'ctrl']; }, '(uniqueItems)'],
  ['unknown modifier', (x) => { x.pages[0].tiles[1].action.mod = 'hyper'; }, '(enum)'],
  ['unknown action', (x) => { x.pages[0].tiles[0].action.type = 'macro'; }, '(oneOf)'],
  ['missing action field', (x) => { delete x.pages[0].tiles[0].action.mods; }, '(required)'],
  ['unknown icon', (x) => { x.pages[0].tiles[0].icon = 'lv:unknown'; }, '(oneOf)'],
  ['short bitmap hash', (x) => { x.pages[0].tiles[0].icon = 'file:123'; }, '(pattern)'],
  ['dashboard with tiles', (x) => { x.pages[3].tiles = []; }, '(additionalProperties)'],
  ['dashboard with grid size', (x) => { x.pages[3].rows = 1; }, '(additionalProperties)'],
  ['dashboard without source', (x) => { delete x.pages[3].source; }, '(required)'],
  ['nonstring command argument', (x) => { x.pages[1].tiles[2].action.args = [1]; }, '(type)'],
  ['nonboolean shell', (x) => { x.pages[1].tiles[2].action.shell = 'cmd'; }, '(type)'],
  ['empty widget account', (x) => { x.pages[2].tiles[0].action.account = ''; }, '(minLength)'],
]) {
  test(`rejects ${name}`, () => {
    const layout = copy();
    mutate(layout);
    assert.ok(validateLayout(layout).some((error) => error.includes(message)), `${name}: ${validateLayout(layout).join('; ')}`);
  });
}

for (const [name, select] of [
  ['layout', (x) => x],
  ['grid page', (x) => x.pages[0]],
  ['dashboard page', (x) => x.pages[3]],
  ['tile', (x) => x.pages[0].tiles[0]],
  ...schema.$defs.action.oneOf.map(({ properties }) => [properties.type.const, (x) => x.pages.flatMap((page) => page.tiles ?? []).find((tile) => tile.action.type === properties.type.const).action]),
]) {
  test(`rejects additional properties on ${name}`, () => {
    const layout = copy();
    select(layout).extra = true;
    assert.ok(validateLayout(layout).some((error) => error.includes('(additionalProperties) "extra"')));
  });
}

test('accepts all inclusive limits, unicode character limits, portrait, and an empty grid', () => {
  const layout = copy();
  layout.orientation = 'portrait';
  layout.pages[0].tiles[0].id = 'a'.repeat(15);
  layout.pages[0].tiles[0].label = '😀'.repeat(16);
  layout.pages[0].title = '😀'.repeat(24);
  layout.pages[0].tiles[0].action.usage = 255;
  layout.pages[0].tiles[0].action.mods = ['ctrl', 'shift', 'alt', 'win'];
  layout.pages[0].tiles[2].action.usage = 65535;
  layout.pages[0].tiles[0].icon = 'file:ABCDEF0123456789';
  layout.pages.push(...Array.from({ length: 4 }, (_, n) => ({ id: `empty_${n}`, title: 'Empty', kind: 'grid', rows: 1, cols: 1, tiles: [] })));
  assert.deepEqual(validateLayout(layout), []);
});

for (const layout of [null, [], 'layout', {}, { ...dev, pages: [null] }, { ...dev, pages: [{ ...dev.pages[0], tiles: [null] }] }]) {
  test(`rejects malformed shape ${JSON.stringify(layout).slice(0, 70)} without throwing`, () => {
    assert.ok(validateLayout(layout).length > 0);
  });
}

test('default checker reports each fixture and the exact acceptance summary', () => {
  const lines = [];
  assert.equal(checkLayouts([], (line) => lines.push(line)), 0);
  assert.equal(lines.filter((line) => line.startsWith('ok ')).length, 14);
  assert.equal(lines.at(-1), '2 valid, 12 invalid as expected, 0 problems');
});

test('compact UTF-8 JSON accepts exactly 32768 bytes and rejects 32769 bytes', () => {
  const layout = copy();
  layout.name = '😀 "\n';
  const initialBytes = Buffer.byteLength(JSON.stringify(layout), 'utf8');
  layout.name += 'x'.repeat(32768 - initialBytes);
  assert.equal(Buffer.byteLength(JSON.stringify(layout), 'utf8'), 32768);
  assert.ok(JSON.stringify(layout).length < 32768);
  assert.deepEqual(validateLayout(layout), []);
  layout.name += 'x';
  assert.equal(Buffer.byteLength(JSON.stringify(layout), 'utf8'), 32769);
  assert.deepEqual(validateLayout(layout), ['layout compact UTF-8 JSON is 32769 bytes; maximum is 32768 bytes']);
});

test('CLI ignores formatting whitespace and counts whitespace inside strings', () => {
  const temporary = mkdtempSync(join(tmpdir(), 'touchdeck-size-'));
  const path = join(temporary, 'boundary.json');
  const layout = copy();
  layout.name = ' ';
  layout.name += ' '.repeat(32768 - Buffer.byteLength(JSON.stringify(layout), 'utf8'));
  const run = () => spawnSync(process.execPath, [resolve(directory, 'check-layouts.mjs'), path], { encoding: 'utf8' });
  try {
    const source = JSON.stringify(layout, null, 2) + '\n';
    assert.ok(Buffer.byteLength(source, 'utf8') > 32768);
    writeFileSync(path, source);
    let result = run();
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.match(result.stdout, /3 valid, 12 invalid as expected, 0 problems/);
    layout.name += ' ';
    writeFileSync(path, JSON.stringify(layout));
    result = run();
    assert.equal(result.status, 1);
    assert.match(result.stdout, /layout compact UTF-8 JSON is 32769 bytes; maximum is 32768 bytes/);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test('CLI checks extra paths, rejects JSON and file errors, and runs from another directory', () => {
  const temporary = mkdtempSync(join(tmpdir(), 'touchdeck-schema-'));
  const run = (...paths) => spawnSync(process.execPath, [resolve(directory, 'check-layouts.mjs'), ...paths], { cwd: temporary, encoding: 'utf8' });
  try {
    const good = join(temporary, 'good.json');
    writeFileSync(good, JSON.stringify(dev));
    let result = run(good);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /3 valid, 12 invalid as expected, 0 problems/);
    assert.match(result.stdout, /wall time: \d+\.\d+ ms/);
    const bad = join(temporary, 'bad.json');
    writeFileSync(bad, readFileSync(join(directory, 'examples/invalid/overlap.json')));
    result = run(bad);
    assert.equal(result.status, 1);
    assert.match(result.stdout, /"alpha" and "beta" overlap/);
    writeFileSync(bad, '{');
    result = run(bad);
    assert.equal(result.status, 1);
    assert.match(result.stdout, /1 problems/);
    result = run(join(temporary, 'missing.json'));
    assert.equal(result.status, 1);
    assert.match(result.stdout, /ENOENT/);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});
