import { truncDiv, type Rect } from './geometry.ts';

/** The now-playing constants of schema/live/now-playing-vectors.json, in Tile pixels. */
export const NOW_PLAYING = { pad: 20, art_max: 256, art_min: 48, row_h: 56, row_min_tile_h: 200, row_min_text_w: 180 } as const;

/** The bar constants of schema/live/bar-vectors.json, in Tile pixels. */
export const BAR = { pad: 20, glyph_size: 32, track_h: 28, capture_expand_y: 24 } as const;

export type Part = 'previous' | 'play_pause' | 'next';

export interface PartZone extends Rect {
  part: Part;
}

export interface NowPlayingLayout {
  /** The cover's square, or null on a Tile too small for one. */
  art: { x: number; y: number; size: number } | null;
  text: { x: number; w: number };
  /** Whether the Tile has the previous, play and next row. Without it, the whole Tile is play_pause. */
  controls: boolean;
  parts: PartZone[];
}

/** Where a now-playing Tile of this size draws its cover, text and controls, as the Firmware and the Companion place them. */
export function nowPlayingLayout(w: number, h: number): NowPlayingLayout {
  const { pad, art_max, art_min, row_h, row_min_tile_h, row_min_text_w } = NOW_PLAYING;
  const size = Math.min(h - 2 * pad, truncDiv((w - 2 * pad) * 2, 5), art_max);
  const art = size >= art_min ? { x: pad, y: truncDiv(h - size, 2), size } : null;
  const x = art ? art.x + art.size + pad : pad;
  const text = { x, w: w - pad - x };
  const controls = h >= row_min_tile_h && text.w >= row_min_text_w;
  if (!controls) return { art, text, controls, parts: [{ part: 'play_pause', x: 0, y: 0, w, h }] };
  const y = h - pad - row_h;
  const each = truncDiv(text.w, 3);
  const parts: PartZone[] = [
    { part: 'previous', x, y, w: each, h: row_h },
    { part: 'play_pause', x: x + each, y, w: each, h: row_h },
    { part: 'next', x: x + 2 * each, y, w: text.w - 2 * each, h: row_h },
  ];
  return { art, text, controls, parts };
}

export interface BarLayout {
  /** The speaker glyph at the top left. */
  glyph: Rect;
  track: Rect;
  /** The glyph's tap zone, which mutes, when the Tile declares `muted`. */
  mute: Rect | null;
}

/** Where a bar Tile of this size draws its speaker glyph and track, as the Firmware places them. */
export function barLayout(w: number, h: number, mutedDeclared: boolean): BarLayout {
  const { pad, glyph_size, track_h } = BAR;
  const glyph = { x: pad, y: pad, w: glyph_size, h: glyph_size };
  return {
    glyph,
    track: { x: pad, y: h - pad - track_h, w: Math.max(w - 2 * pad, 0), h: track_h },
    mute: mutedDeclared ? glyph : null,
  };
}
