/** The most UTF-8 bytes of live text a Device takes: the `text_bytes` cap of schema/protocol/link-v2.md. */
export const TEXT_BYTES = 128;

const LATIN_EXTENDED_A = [
  'A', 'a', 'A', 'a', 'A', 'a', 'C', 'c', 'C', 'c', 'C', 'c', 'C', 'c', 'D', 'd', 'D', 'd', 'E', 'e', 'E', 'e', 'E', 'e', 'E', 'e', 'E', 'e', 'G', 'g', 'G', 'g',
  'G', 'g', 'G', 'g', 'H', 'h', 'H', 'h', 'I', 'i', 'I', 'i', 'I', 'i', 'I', 'i', 'I', 'i', 'IJ', 'ij', 'J', 'j', 'K', 'k', 'k', 'L', 'l', 'L', 'l', 'L', 'l', 'L',
  'l', 'L', 'l', 'N', 'n', 'N', 'n', 'N', 'n', "'n", 'N', 'n', 'O', 'o', 'O', 'o', 'O', 'o', 'OE', 'oe', 'R', 'r', 'R', 'r', 'R', 'r', 'S', 's', 'S', 's', 'S', 's',
  'S', 's', 'T', 't', 'T', 't', 'T', 't', 'U', 'u', 'U', 'u', 'U', 'u', 'U', 'u', 'U', 'u', 'U', 'u', 'W', 'w', 'Y', 'y', 'Y', 'Z', 'z', 'Z', 'z', 'Z', 'z', 's',
] as const;

const COMMA_BELOW: Partial<Record<number, string>> = { 0x218: 'S', 0x219: 's', 0x21a: 'T', 0x21b: 't' };

const inRange = (cp: number, low: number, high: number) => cp >= low && cp <= high;

function fold(c: string): string {
  const cp = c.codePointAt(0) ?? 0;
  if (cp <= 0x1f || inRange(cp, 0x7f, 0x9f) || cp === 0x2028 || cp === 0x2029) return ' ';
  if (inRange(cp, 0x200b, 0x200d) || cp === 0x2060 || cp === 0xfeff || inRange(cp, 0xfe00, 0xfe0f) || inRange(cp, 0x300, 0x36f)) return '';
  if (inRange(cp, 0x100, 0x17f)) return LATIN_EXTENDED_A[cp - 0x100];
  const romanian = COMMA_BELOW[cp];
  if (romanian) return romanian;
  if (inRange(cp, 0x2010, 0x2012) || cp === 0x2015) return '\u2013';
  if (inRange(cp, 0x2000, 0x200a) || cp === 0x202f || cp === 0x205f || cp === 0x3000) return ' ';
  if (cp === 0x2039) return '<';
  if (cp === 0x203a) return '>';
  return c;
}

const WHITE_SPACE = /[\t-\r \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]+/u;

const utf8Bytes = (c: string) => {
  const cp = c.codePointAt(0) ?? 0;
  return cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
};

/**
 * Text an Integration reports, made fit for a Device that draws `maxBytes` of UTF-8 in the glyph set of docs/design.md
 * (Fonts on the Device), as td_core::live::fit_text does it and schema/live/text-vectors.json checks.
 */
export function fitText(text: string, maxBytes: number): string {
  let folded = '';
  for (const c of text.normalize('NFC')) folded += fold(c);
  let fitted = '';
  let bytes = 0;
  for (const word of folded.split(WHITE_SPACE)) {
    if (word === '') continue;
    const gap = bytes > 0 ? 1 : 0;
    if (bytes + gap >= maxBytes) break;
    if (gap) {
      fitted += ' ';
      bytes += 1;
    }
    for (const c of word) {
      const size = utf8Bytes(c);
      if (bytes + size > maxBytes) return fitted.trimEnd();
      fitted += c;
      bytes += size;
    }
  }
  return fitted.trimEnd();
}
