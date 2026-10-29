// How LVGL measures and breaks a label's text, ported from LVGL 9's lv_text.c, lv_font.c, lv_font_fmt_txt.c and
// lv_label.c with the firmware's settings: LV_TXT_BREAK_CHARS " ,.;:-_)]}", LV_TXT_LINE_BREAK_LONG_LEN 0, no letter
// or line space, and the glyph placeholder on. Text is a list of code points, as LVGL walks UTF-8.

export interface FontData {
  size: number;
  lineHeight: number;
  baseLine: number;
  kernScale: number;
  rightClasses: number;
  glyphs: [codepoint: number, advance: number, left: number, right: number][];
  kerning: string;
}

interface Glyph {
  advance: number;
  left: number;
  right: number;
}

export interface Font {
  size: number;
  lineHeight: number;
  baseLine: number;
  has(letter: number): boolean;
  /** A letter's advance in whole pixels with its kerning against the next letter, as lv_font_get_glyph_width. */
  width(letter: number, next: number): number;
}

function decode(base64: string): Int8Array {
  const bytes = typeof atob === 'function' ? Uint8Array.from(atob(base64), (c) => c.charCodeAt(0)) : new Uint8Array(Buffer.from(base64, 'base64'));
  return new Int8Array(bytes.buffer);
}

const BREAK_CHARS = new Set([...' ,.;:-_)]}'].map((c) => c.codePointAt(0)!));
const SPACE = 0x20;
const NL = 0x0a;
const CR = 0x0d;
const DOT = 0x2e;
const DOT_NUM = 3;

export function isMarker(letter: number): boolean {
  return (
    letter < 0x20 ||
    letter === 0x061c ||
    letter === 0x115f ||
    letter === 0x1160 ||
    (letter >= 0x180b && letter <= 0x180e) ||
    (letter >= 0x200b && letter <= 0x200f) ||
    (letter >= 0x2028 && letter <= 0x202f) ||
    (letter >= 0x205f && letter <= 0x206f) ||
    letter === 0xfeff ||
    letter === 0xf8ff
  );
}

export function font(data: FontData): Font {
  const glyphs = new Map<number, Glyph>(data.glyphs.map(([cp, advance, left, right]) => [cp, { advance, left, right }]));
  const pairs = decode(data.kerning);
  const placeholder = Math.trunc(data.lineHeight / 2) + 2;
  return {
    size: data.size,
    lineHeight: data.lineHeight,
    baseLine: data.baseLine,
    has: (letter) => glyphs.has(letter),
    width(letter, next) {
      if (isMarker(letter)) return 0;
      const glyph = glyphs.get(letter);
      if (!glyph) return placeholder;
      let kern = 0;
      const after = next ? glyphs.get(next) : undefined;
      if (after && glyph.left > 0 && after.right > 0) kern = pairs[(glyph.left - 1) * data.rightClasses + (after.right - 1)];
      const advance = Math.max(0, glyph.advance + ((kern * data.kernScale) >> 4));
      return (advance + 8) >> 4;
    },
  };
}

const isBreak = (letter: number) => BREAK_CHARS.has(letter);
const isHanging = (letter: number) => letter === SPACE || letter === NL || letter === CR;

/** CJK and kana count as one word each, as lv_text_is_a_word. */
function isWord(letter: number): boolean {
  if (letter === 0) return false;
  return (
    (letter >= 0x4e00 && letter <= 0x9fff) ||
    (letter >= 0xff01 && letter <= 0xff5e) ||
    (letter >= 0x3000 && letter <= 0x303f) ||
    (letter >= 0x2e80 && letter <= 0x2eff) ||
    (letter >= 0x31c0 && letter <= 0x31ef) ||
    (letter >= 0x3040 && letter <= 0x30ff) ||
    (letter >= 0xfe10 && letter <= 0xfe1f) ||
    (letter >= 0xfe30 && letter <= 0xfe4f)
  );
}

export interface Flags {
  /** LV_TEXT_FLAG_EXPAND or LV_TEXT_FLAG_FIT: break only at new lines. */
  fit?: boolean;
  breakAll?: boolean;
}

const NO_BREAK = -1;
const at = (text: number[], i: number) => (i < text.length ? text[i] : 0);

/** The length of the next word from `start` and its width, as lv_text_get_next_word. */
function nextWord(text: number[], start: number, f: Font, maxWidth: number, flags: Flags): { advance: number; width: number } {
  if (at(text, start) === 0) return { advance: 0, width: 0 };
  let i = start;
  let width = 0;
  let current = 0;
  let wordLength = 0;
  let breakIndex = NO_BREAK;
  let letter = at(text, i);
  let next = 0;
  while (at(text, i) !== 0) {
    next = at(text, i + 1);
    wordLength++;
    const letterWidth = f.width(letter, next);
    current += letterWidth;
    const breaks = letter === NL || letter === CR || isBreak(letter);
    if (!isHanging(letter) && breakIndex === NO_BREAK && current > maxWidth) {
      breakIndex = i;
      if (flags.breakAll) break;
    }
    if (breaks) {
      if (i === start && breakIndex === NO_BREAK) width = current;
      wordLength--;
      break;
    } else if (isWord(next) || isWord(letter)) {
      width = current;
      i = i + 1;
      break;
    }
    if (breakIndex === NO_BREAK) width = current;
    i = i + 1;
    letter = next;
  }
  if (breakIndex === NO_BREAK) {
    if (wordLength === 0 || (letter === CR && next === NL)) i = i + 1;
    return { advance: i - start, width };
  }
  if (flags.breakAll) return { advance: breakIndex - start, width };
  return { advance: 0, width: 0 };
}

const spaceRun = (text: number[], start: number, length: number) =>
  length > 0 && text.slice(start, start + length).every((letter) => letter === SPACE);

/** The length of the line that starts at `start`, as lv_text_get_next_line. */
export function nextLine(text: number[], start: number, f: Font, maxWidth: number, flags: Flags = {}): number {
  if (at(text, start) === 0) return 0;
  if (flags.fit) {
    let i = start;
    while (i < text.length && text[i] !== NL && text[i] !== CR) i++;
    if (i < text.length) i++;
    return i - start;
  }
  let i = start;
  let room = maxWidth;
  let hanging = 0;
  let explicit = false;
  while (i < text.length && room > hanging) {
    const word = nextWord(text, i, f, room - hanging, i === start ? { ...flags, breakAll: true } : flags);
    if (word.advance === 0) break;
    if (spaceRun(text, i, word.advance)) {
      hanging += word.width;
    } else {
      room -= word.width + hanging;
      hanging = 0;
    }
    i += word.advance;
    if (text[start] === NL || text[start] === CR) {
      explicit = true;
      break;
    }
    if (at(text, i) === NL || at(text, i) === CR) {
      i++;
      explicit = true;
      break;
    }
  }
  if (i === start) i++;
  if (!explicit) while (i < text.length && text[i] === SPACE) i++;
  return i - start;
}

/** The width of `length` letters from `start`, as lv_text_get_width. */
export function textWidth(text: number[], start: number, length: number, f: Font): number {
  let width = 0;
  for (let i = start; i < start + length && i < text.length; i++) width += f.width(text[i], at(text, i + 1));
  return width;
}

/** The width of a line without the spaces that hang off its end, as lv_text_get_line_width. */
export function lineWidth(text: number[], start: number, length: number, f: Font): number {
  let end = Math.min(start + length, text.length);
  while (end > start && isHanging(text[end - 1])) end--;
  return textWidth(text, start, end - start, f);
}

/** The width of the longest line and the height of all lines, as lv_text_get_size. */
export function textSize(text: number[], f: Font, maxWidth: number, flags: Flags = {}): { w: number; h: number } {
  let start = 0;
  let w = 0;
  let h = 0;
  while (at(text, start) !== 0) {
    const length = nextLine(text, start, f, maxWidth, flags);
    h += f.lineHeight;
    w = Math.max(w, lineWidth(text, start, length, f));
    start += length;
  }
  if (start !== 0 && (text[start - 1] === NL || text[start - 1] === CR)) h += f.lineHeight;
  return { w, h: h === 0 ? f.lineHeight : h };
}

export const codepoints = (text: string) => Array.from(text, (c) => c.codePointAt(0)!);

export interface Line {
  x: number;
  y: number;
  letters: { letter: number; x: number; width: number; placeholder: boolean }[];
}

export type Align = 'left' | 'center';

/**
 * A label as LVGL draws it. `width` and `height` are the label's own size, or null for LV_SIZE_CONTENT, which
 * measures the text on one line per new line. `dots` is LV_LABEL_LONG_MODE_DOTS, which ends the last line that fits
 * with three dots when the text is taller than the label.
 */
export interface Label {
  width: number;
  height: number;
  lines: Line[];
}

export function layoutLabel(text: string, f: Font, options: { width?: number | null; height?: number | null; align?: Align; dots?: boolean } = {}): Label {
  let letters = codepoints(text);
  const fixedWidth = options.width ?? null;
  const flags: Flags = fixedWidth === null ? { fit: true } : {};
  const maxWidth = fixedWidth ?? Number.MAX_SAFE_INTEGER;
  const size = textSize(letters, f, maxWidth, flags);
  const width = fixedWidth ?? size.w;
  const height = options.height ?? size.h;
  if (options.dots && size.h > height && size.h > f.lineHeight && letters.length > DOT_NUM) {
    letters = withDots(letters, f, width, height, options.align ?? 'left');
  }
  const lines: Line[] = [];
  let start = 0;
  let y = 0;
  while (at(letters, start) !== 0 && y < height) {
    const length = nextLine(letters, start, f, maxWidth, flags);
    const lineW = lineWidth(letters, start, length, f);
    const x = options.align === 'center' ? Math.trunc((width - lineW) / 2) : 0;
    let cursor = 0;
    const placed = [];
    for (let i = start; i < start + length; i++) {
      const letter = letters[i];
      const letterWidth = f.width(letter, at(letters, i + 1));
      if (letter !== NL && letter !== CR) placed.push({ letter, x: cursor, width: letterWidth, placeholder: !isMarker(letter) && !f.has(letter) });
      cursor += letterWidth;
    }
    lines.push({ x, y, letters: placed });
    start += length;
    y += f.lineHeight;
  }
  return { width, height, lines };
}

const utf8Length = (letter: number) => (letter < 0x80 ? 1 : letter < 0x800 ? 2 : letter < 0x10000 ? 3 : 4);

/** The text with its tail replaced by dots, as lv_label_refr_text's LV_LABEL_LONG_MODE_DOTS branch. */
function withDots(letters: number[], f: Font, width: number, height: number, align: Align): number[] {
  const x = width - f.width(DOT, DOT) * DOT_NUM;
  let y = height;
  const over = y % f.lineHeight;
  y = over >= f.lineHeight ? y - over + f.lineHeight : y - over;
  let letter = letterOn(letters, f, width, height, align, x, y);
  const bytes = letters.reduce((sum, l) => sum + utf8Length(l), 0);
  let byte = letters.slice(0, letter).reduce((sum, l) => sum + utf8Length(l), 0);
  while (byte + DOT_NUM > bytes && letter > 0) {
    letter--;
    byte -= utf8Length(letters[letter]);
  }
  return [...letters.slice(0, letter), DOT, DOT, DOT];
}

/** The letter under a point of a fixed-size dots label, as lv_label_get_letter_on. */
function letterOn(letters: number[], f: Font, width: number, height: number, align: Align, px: number, py: number): number {
  let lineStart = 0;
  let newLineStart = 0;
  let y = 0;
  while (at(letters, lineStart) !== 0) {
    const last = y + f.lineHeight + f.lineHeight > height;
    newLineStart += nextLine(letters, lineStart, f, width, last ? { breakAll: true } : {});
    if (py <= y + f.lineHeight) {
      if (at(letters, newLineStart - 1) !== NL && at(letters, newLineStart) === 0) newLineStart++;
      break;
    }
    y += f.lineHeight;
    lineStart = newLineStart;
  }
  let x = align === 'center' ? Math.trunc(width / 2) - Math.trunc(lineWidth(letters, lineStart, newLineStart - lineStart, f) / 2) : 0;
  let i = 0;
  let act = 0;
  if (newLineStart > 0) {
    while (i + lineStart < newLineStart) {
      const letter = at(letters, lineStart + i);
      const glyph = f.width(letter, at(letters, lineStart + i + 1));
      i++;
      if (px < x + glyph || i + lineStart === newLineStart || at(letters, act + lineStart) === 0) {
        i = act;
        break;
      }
      x += glyph;
      act = i;
    }
  }
  return i + lineStart;
}
