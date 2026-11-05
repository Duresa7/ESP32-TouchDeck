import assert from 'node:assert/strict';
import { test } from 'vitest';
import { ARCHIVO_MEDIUM_22, ARCHIVO_MEDIUM_26, ARCHIVO_MEDIUM_44, MONTSERRAT_48 } from './fonts.data.ts';
import { codepoints, font, layoutLabel, nextLine, textSize } from './text.ts';

const f44 = font(ARCHIVO_MEDIUM_44);
const f26 = font(ARCHIVO_MEDIUM_26);
const f22 = font(ARCHIVO_MEDIUM_22);
const icons = font(MONTSERRAT_48);
const width = (text: string, f = f26) => textSize(codepoints(text), f, Number.MAX_SAFE_INTEGER).w;

test('the fonts carry the LVGL line heights and the deck character set', () => {
  assert.deepEqual([f44.lineHeight, f26.lineHeight, f22.lineHeight, icons.lineHeight], [42, 30, 26, 52]);
  assert.deepEqual([f44.baseLine, f26.baseLine, f22.baseLine, icons.baseLine], [9, 5, 5, 9]);
  for (const c of ' ~°•é…−') assert.ok(f26.has(c.codePointAt(0)!), c);
  assert.ok(icons.has(0xf04b));
  assert.equal(f44.has('é'.codePointAt(0)!), false);
  assert.equal(f26.has('€'.codePointAt(0)!), false);
});

test('an advance rounds the 1/16 px width with its kerning, letter by letter', () => {
  const F = ARCHIVO_MEDIUM_44.glyphs.find(([cp]) => cp === 70)!;
  const one = ARCHIVO_MEDIUM_44.glyphs.find(([cp]) => cp === 49)!;
  assert.equal(f44.width(70, 0), (F[1] + 8) >> 4);
  assert.equal(width('F1', f44), ((F[1] + 8) >> 4) + ((one[1] + 8) >> 4));
  assert.ok(width('AV') < width('A') + width('V'), 'A and V kern together by class');
});

test('a font kerned by glyph pairs scales the pair value by its kern scale', () => {
  const A = ARCHIVO_MEDIUM_44.glyphs.find(([cp]) => cp === 65)!;
  const pair = ARCHIVO_MEDIUM_44.pairs!.find(([left, right]) => left === 65 && right === 84)!;
  assert.equal(f44.width(65, 84), (A[1] + ((pair[2] * ARCHIVO_MEDIUM_44.kernScale) >> 4) + 8) >> 4);
  assert.ok(width('AT', f44) < width('A', f44) + width('T', f44), 'A and T kern together by pair');
  assert.equal(f44.width(65, 65), (A[1] + 8) >> 4);
});

test('a missing glyph is a placeholder box half a line wide', () => {
  assert.equal(f26.width('€'.codePointAt(0)!, 0), 17);
  const label = layoutLabel('€', f26);
  assert.equal(label.lines[0].letters[0].placeholder, true);
});

test('a line breaks after a space and keeps the space off the next line', () => {
  const text = codepoints('Open browser');
  const first = nextLine(text, 0, f26, width('Open browse'));
  assert.equal(first, 5);
  const label = layoutLabel('Open browser', f26, { width: width('Open browse'), align: 'center' });
  assert.deepEqual(label.lines.map((line) => line.letters.map((l) => String.fromCodePoint(l.letter)).join('')), ['Open ', 'browser']);
  assert.equal(label.height, 60);
});

test('a word wider than the line breaks inside the word', () => {
  const label = layoutLabel('Supercalifragil', f22, { width: 40, align: 'center' });
  assert.ok(label.lines.length > 1);
  assert.equal(label.lines.map((line) => line.letters.map((l) => String.fromCodePoint(l.letter)).join('')).join(''), 'Supercalifragil');
});

test('a dots label taller than its box ends its last line with three dots', () => {
  const label = layoutLabel('Launch my very long app name', f22, { width: 40, height: 52, align: 'center', dots: true });
  const shown = label.lines.map((line) => line.letters.map((l) => String.fromCodePoint(l.letter)).join(''));
  assert.ok(shown.join('').endsWith('...'), shown.join('|'));
  assert.ok(label.lines.length >= 2);
});
