import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MONTSERRAT_14, MONTSERRAT_28, MONTSERRAT_48 } from './fonts.data.ts';
import { codepoints, font, layoutLabel, nextLine, textSize } from './text.ts';

const f48 = font(MONTSERRAT_48);
const f28 = font(MONTSERRAT_28);
const f14 = font(MONTSERRAT_14);
const width = (text: string, f = f28) => textSize(codepoints(text), f, Number.MAX_SAFE_INTEGER).w;

test('the fonts carry the LVGL line heights and the deck character set', () => {
  assert.deepEqual([f48.lineHeight, f28.lineHeight, f14.lineHeight], [52, 30, 16]);
  assert.deepEqual([f48.baseLine, f28.baseLine, f14.baseLine], [9, 5, 3]);
  for (const c of ' ~°•') assert.ok(f28.has(c.codePointAt(0)!), c);
  assert.ok(f48.has(0xf04b));
  assert.equal(f28.has('é'.codePointAt(0)!), false);
});

test('an advance rounds the 1/16 px width with its kerning, letter by letter', () => {
  // lv_font_montserrat_48.c: F is 488/16 px and 1 is 284/16 px wide, and the pair F 1 has no kerning.
  const F = MONTSERRAT_48.glyphs.find(([cp]) => cp === 70)!;
  const one = MONTSERRAT_48.glyphs.find(([cp]) => cp === 49)!;
  assert.equal(f48.width(70, 0), (F[1] + 8) >> 4);
  assert.equal(width('F1', f48), ((F[1] + 8) >> 4) + ((one[1] + 8) >> 4));
  assert.ok(width('AV') < width('A') + width('V'), 'A and V kern together');
});

test('a missing glyph is a placeholder box half a line wide', () => {
  assert.equal(f28.width('é'.codePointAt(0)!, 0), 17);
  const label = layoutLabel('é', f28);
  assert.equal(label.lines[0].letters[0].placeholder, true);
});

test('a line breaks after a space and keeps the space off the next line', () => {
  const text = codepoints('Open browser');
  const first = nextLine(text, 0, f28, width('Open browse'));
  assert.equal(first, 5);
  const label = layoutLabel('Open browser', f28, { width: width('Open browse'), align: 'center' });
  assert.deepEqual(label.lines.map((line) => line.letters.map((l) => String.fromCodePoint(l.letter)).join('')), ['Open ', 'browser']);
  assert.equal(label.height, 60);
});

test('a word wider than the line breaks inside the word', () => {
  const label = layoutLabel('Supercalifragil', f14, { width: 40, align: 'center' });
  assert.ok(label.lines.length > 1);
  assert.equal(label.lines.map((line) => line.letters.map((l) => String.fromCodePoint(l.letter)).join('')).join(''), 'Supercalifragil');
});

test('a dots label taller than its box ends its last line with three dots', () => {
  const label = layoutLabel('Launch my very long app name', f14, { width: 40, height: 32, align: 'center', dots: true });
  const shown = label.lines.map((line) => line.letters.map((l) => String.fromCodePoint(l.letter)).join(''));
  assert.ok(shown.join('').endsWith('...'), shown.join('|'));
  assert.ok(label.lines.length >= 2);
});
