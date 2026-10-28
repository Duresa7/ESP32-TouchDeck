import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'vitest';
import type { Layout } from '../editor/validate.ts';
import { geometry, screenSize, tileRect } from './geometry.ts';
import { buildDots, buildPage, buildTitle, fitLabel, FONTS, missingGlyphs, sampleRows, sampleWidget } from './scene.ts';
import { layoutLabel } from './text.ts';
import { panelColor, THEME } from './theme.ts';

const examples = join(import.meta.dirname, '..', '..', '..', '..', '..', 'schema', 'examples');
const load = (name: string) => JSON.parse(readFileSync(join(examples, name), 'utf8')) as Layout;
const landscape = geometry(screenSize('landscape'));
const shown = (view: { label: { lines: { letters: { letter: number }[] }[] } }) =>
  view.label.lines.map((line) => line.letters.map((l) => String.fromCodePoint(l.letter)).join(''));

test('a short label keeps the 48 px tile font and sits in the middle of its tile', () => {
  const page = buildPage(load('builtin.json').pages[0], landscape);
  const f1 = page.tiles.find((tile) => tile.id === 'f1')!;
  assert.deepEqual(f1.rect, tileRect({ rows: 4, cols: 4 }, { row: 0, col: 0, rowSpan: 1, colSpan: 1 }, landscape.grid, landscape.gap));
  const [label] = f1.texts;
  assert.equal(label.size, 48);
  assert.equal(label.color, THEME.color.text);
  assert.equal(label.y, Math.trunc(f1.rect.h / 2) - Math.trunc(52 / 2));
  assert.equal(label.x, Math.trunc(f1.rect.w / 2) - Math.trunc(label.label.width / 2));
});

test('a label steps down to 28 px, then 14 px, then wraps with dots, as fit_label', () => {
  assert.equal(fitLabel('Shift', 200, 100).size, 48);
  // Ctrl+Shift+F13 is 331 px wide at 48 px, 192 px at 28 px, and 98 px at 14 px, against the tile less 32 px.
  assert.equal(fitLabel('Ctrl+Shift+F13', 240, 100).size, 28);
  assert.equal(fitLabel('Ctrl+Shift+F13', 140, 100).size, 14);
  assert.equal(fitLabel('Ctrl+Shift+F13', 120, 100).label.lines.length, 2);
  const wrapped = fitLabel('Launch the thing', 54, 97);
  assert.equal(wrapped.size, 14);
  assert.equal(wrapped.label.width, 54 - 32);
  assert.ok(wrapped.label.lines.length > 1);
});

test('a media tile stacks its symbol over a one-line caption', () => {
  const page = buildPage(load('builtin.json').pages[1], landscape);
  const play = page.tiles.find((tile) => tile.id === 'playpause')!;
  const [caption, icon] = play.texts;
  assert.equal(caption.size, 28);
  assert.equal(caption.label.height, 30);
  assert.equal(caption.label.width, play.rect.w - 32);
  assert.equal(caption.y, Math.trunc(play.rect.h / 2) - 15 + Math.trunc((52 + 14) / 2));
  assert.deepEqual(shown(icon), ['']);
  assert.equal(icon.y, Math.trunc(play.rect.h / 2) - 26 - 22);
});

test('a widget tile shows its value, its caption below, and a bar as wide as the reading', () => {
  const page = buildPage(load('dev.json').pages[2], landscape, { widget: sampleWidget });
  const five = page.tiles.find((tile) => tile.id === 'five_hour')!;
  const [value, caption] = five.texts;
  assert.deepEqual(shown(value), ['42%']);
  assert.deepEqual(shown(caption), ['5h usage']);
  assert.equal(caption.color, THEME.color.caption);
  assert.deepEqual(five.bar, { x: 0, y: five.rect.h - 18, w: Math.trunc(0.42 * five.rect.w + 0.5), h: 6, radius: 3 });
  assert.equal(five.dims, true);
  const bare = buildPage(load('dev.json').pages[2], landscape).tiles[0];
  assert.deepEqual(shown(bare.texts[0]), ['--']);
  assert.equal(bare.bar, null);
});

test('a widget tile keeps its caption and value on one line, cut with dots to the tile', () => {
  const usage = load('dev.json').pages[2] as Layout['pages'][number] & { kind: 'grid' };
  const page = { ...usage, cols: 4, tiles: [{ ...usage.tiles[0], label: 'Claude 5h usage' }] };
  const long = buildPage(page, landscape, { widget: () => ({ value: '100% of the five hour', caption: 'Claude 5h usage', bar: -1 }) });
  const [value, caption] = long.tiles[0].texts;
  const maxW = long.tiles[0].rect.w - 32;
  assert.equal(caption.label.lines.length, 1);
  assert.equal(caption.label.width, maxW);
  assert.equal(caption.label.height, 30);
  assert.match(shown(caption)[0], /^Claude .*\.\.\.$/);
  assert.equal(caption.y, Math.trunc(long.tiles[0].rect.h / 2) - 15 + 30);
  assert.equal(value.label.lines.length, 1);
  assert.equal(value.label.width, maxW);
  assert.match(shown(value)[0], /\.\.\.$/);
  const short = buildPage(page, landscape, { widget: () => ({ value: '42%', caption: '5h', bar: -1 }) });
  const [fit, fitCaption] = short.tiles[0].texts;
  assert.deepEqual(shown(fit), ['42%']);
  assert.ok(fit.label.width < maxW);
  assert.equal(fit.x, Math.trunc(short.tiles[0].rect.w / 2) - Math.trunc(fit.label.width / 2));
  assert.deepEqual(shown(fitCaption), ['5h']);
});

test('a dashboard page lays its rows down the grid, 8 rows tall', () => {
  const page = buildPage(load('dev.json').pages[3], landscape, { rows: sampleRows });
  assert.equal(page.rows.length, 3);
  const rowH = Math.trunc((landscape.grid.h - 7 * 8) / 8);
  page.rows.forEach((row, i) => assert.deepEqual(row.rect, { x: 28, y: 76 + i * (rowH + 8), w: 968, h: rowH }));
  assert.equal(page.rows[0].texts[1].x, landscape.dashValueX);
  assert.equal(page.rows[0].bar!.x, landscape.dashBarX);
  assert.equal(buildPage(load('dev.json').pages[3], landscape).rows.length, 0);
});

test('a dashboard row wraps its text only as far as the row holds, the last line cut with dots', () => {
  const dashboard = load('dev.json').pages[3];
  const rows = () => [
    { label: 'claude jordan.k', value: '5h 100% 7d 100%', bar: 1 },
    { label: 'WWWWWWWWWWWWWWWW', value: '0000000000000000', bar: -1 },
  ];
  const uncut = (value: string, g: typeof landscape) => layoutLabel(value, FONTS[28], { width: g.dashValueX - 2 * THEME.dash.pad });

  // A landscape row is 51 px, one 30 px line, so a value wider than its 220 px column stays on one line.
  const wide = buildPage(dashboard, landscape, { rows });
  const [label, value] = wide.rows[0].texts;
  assert.deepEqual(label.label, uncut('claude jordan.k', landscape));
  assert.equal(label.y, 25 - 15);
  assert.deepEqual(shown(value), ['5h 100% 7d 1...']);
  assert.deepEqual([value.label.width, value.label.height, value.y], [220, 30, 25 - 15]);
  assert.deepEqual(shown(wide.rows[1].texts[0]), ['WWWWWWWWW...']);

  // A portrait row is 104 px, three lines. Text on two lines wraps as it always did, and a fourth line is cut.
  const portrait = geometry(screenSize('portrait'));
  const tall = buildPage(dashboard, portrait, { rows });
  const [twoLines, twoValues] = tall.rows[0].texts;
  assert.deepEqual(twoLines.label, uncut('claude jordan.k', portrait));
  assert.deepEqual(shown(twoValues), ['5h 100% ', '7d 100%']);
  assert.equal(twoValues.y, 52 - 30);
  const cut = tall.rows[1].texts[0];
  assert.equal(uncut('WWWWWWWWWWWWWWWW', portrait).lines.length, 4);
  assert.deepEqual([cut.label.height, cut.y, cut.label.lines.length], [90, 52 - 45, 3]);
  assert.match(shown(cut)[2], /^W+\.\.\.$/);

  // A full-screen landscape row is 68 px, so two lines fit there.
  const full = buildPage(dashboard, geometry(screenSize('landscape'), true), { rows });
  assert.deepEqual(shown(full.rows[1].texts[1]), ['000000000000', '0000']);
});

test('the title and dots follow the page, and a full-screen layout draws neither', () => {
  const title = buildTitle('Media', landscape)!;
  assert.deepEqual([title.x, title.y, title.size, title.color], [36, 14, 28, THEME.color.title]);
  const dots = buildDots(landscape, 2, 1);
  assert.deepEqual(dots[0], { x: 495 - 6, y: 580 - 6, w: 12, h: 12, active: false });
  assert.deepEqual(dots[1], { x: 529 - 6 - 11, y: 574, w: 34, h: 12, active: true });
  const full = geometry(screenSize('landscape'), true);
  assert.equal(buildTitle('Media', full), null);
  assert.deepEqual(buildDots(full, 2, 0), []);
});

test('a title too wide for the screen stays on one line, cut with dots short of the right margin', () => {
  const portrait = geometry(screenSize('portrait'));
  const fits = buildTitle('Usage on every account', portrait)!;
  assert.deepEqual(fits.label.lines, layoutLabel('Usage on every account', FONTS[28]).lines);
  assert.deepEqual([fits.x, fits.y, fits.label.height], [36, 14, 30]);
  // 24 W are 768 px, wider than the 528 px between the margins of a portrait screen.
  const wide = buildTitle('W'.repeat(24), portrait)!;
  assert.equal(wide.label.width, 600 - 2 * 36);
  assert.deepEqual(shown(wide), [`${'W'.repeat(15)}...`]);
  assert.deepEqual(shown(buildTitle('W'.repeat(24), landscape)!), ['W'.repeat(24)]);
});

test('characters outside the deck font are listed once', () => {
  assert.deepEqual(missingGlyphs('Café ☕ é'), ['é', '☕']);
  assert.deepEqual(missingGlyphs('Play/Pause 100°'), []);
});

test('colors widen from RGB565 the way a deck screenshot reads them', () => {
  // Board screenshots read the tile as 24, 32, 49 and the background as 8, 8, 16.
  assert.equal(THEME.color.tile, '#182031');
  assert.equal(THEME.color.bg, '#080810');
  assert.equal(panelColor(72, 160, 255), '#4aa2ff');
});
