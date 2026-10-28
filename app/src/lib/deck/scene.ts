// What the deck draws for a page, ported from firmware/components/ui: build_tile, fit_label and stack_icon in
// ui_deck.c, ui_widget_build and draw_widget in ui_widget.c, and ui_dashboard_build in ui_dashboard.c. Positions are
// screen pixels, and text is laid out by the LVGL port in text.ts with the deck's own font metrics.
import type { Action, Page, Tile } from '../editor/validate.ts';
import { MONTSERRAT_14, MONTSERRAT_28, MONTSERRAT_48 } from './fonts.data.ts';
import { div, dotX, tileRect, type Geometry, type Rect } from './geometry.ts';
import { font, layoutLabel, textSize, codepoints, type Align, type Font, type Label } from './text.ts';
import { SYMBOLS, THEME } from './theme.ts';

export type FontSize = 14 | 28 | 48;

export const FONTS: Record<FontSize, Font> = { 48: font(MONTSERRAT_48), 28: font(MONTSERRAT_28), 14: font(MONTSERRAT_14) };

/** LV_COORD_MAX, the width fit_label measures a single line against. */
const COORD_MAX = (1 << 29) - 1;

export interface TextView {
  /** The label's box, relative to its parent. */
  x: number;
  y: number;
  size: FontSize;
  color: string;
  opacity: number;
  label: Label;
}

export interface BarView {
  x: number;
  y: number;
  w: number;
  h: number;
  radius: number;
}

export interface TileView {
  id: string;
  rect: Rect;
  action: Action['type'];
  /** App and widget tiles, which the deck dims while no app is connected. */
  dims: boolean;
  texts: TextView[];
  image: { id: string; x: number; y: number; size: number } | null;
  bar: BarView | null;
}

export interface RowView {
  rect: Rect;
  texts: TextView[];
  bar: BarView | null;
}

export interface PageView {
  id: string;
  title: string;
  kind: Page['kind'];
  tiles: TileView[];
  rows: RowView[];
}

export interface DotView {
  x: number;
  y: number;
  w: number;
  h: number;
  active: boolean;
}

/** A widget tile's or dashboard row's values, as the app sends them in widget.set and dashboard.set. */
export interface WidgetSample {
  value: string;
  caption?: string;
  bar: number;
  stale?: boolean;
}

export interface RowSample {
  label: string;
  value: string;
  bar: number;
}

export interface SceneOptions {
  /** The values a widget tile shows. Without one the tile shows `--` and its label, as before the first update. */
  widget?: (tile: Tile) => WidgetSample | null;
  /** The rows a dashboard page shows. Without any the page shows only its title, as before the first update. */
  rows?: (page: Page) => RowSample[];
  /** Whether a `file:` icon's image is available, so the tile stacks it over its label. */
  hasImage?: (id: string) => boolean;
}

/** LV_ALIGN_CENTER inside a parent of pw by ph, with C's integer halves. */
const centre = (pw: number, ph: number, w: number, h: number, dx = 0, dy = 0) => ({
  x: div(pw, 2) - div(w, 2) + dx,
  y: div(ph, 2) - div(h, 2) + dy,
});

function text(label: Label, size: FontSize, color: string, at: { x: number; y: number }, opacity = 255): TextView {
  return { x: at.x, y: at.y, size, color, opacity, label };
}

/** fit_label: the tile font when the label fits, else the caption then the small font, else wrapped with dots. */
export function fitLabel(value: string, tileW: number, tileH: number): { label: Label; size: FontSize } {
  const letters = codepoints(value);
  const big = textSize(letters, FONTS[48], COORD_MAX);
  if (big.w <= tileW && big.h <= tileH) return { label: layoutLabel(value, FONTS[48]), size: 48 };
  const maxW = tileW - 2 * THEME.media.labelInset;
  for (const size of [28, 14] as const) {
    const measured = textSize(letters, FONTS[size], COORD_MAX);
    if (measured.w <= maxW && measured.h <= tileH) return { label: layoutLabel(value, FONTS[size]), size };
  }
  const wrapped = textSize(letters, FONTS[14], maxW);
  return {
    label: layoutLabel(value, FONTS[14], { width: maxW, height: wrapped.h > tileH ? tileH : null, align: 'center', dots: true }),
    size: 14,
  };
}

const fileId = (icon: string | null) => (icon?.startsWith('file:') ? icon.slice(5) : null);

function buildTile(page: Page & { kind: 'grid' }, tile: Tile, g: Geometry, options: SceneOptions): TileView {
  const rect = tileRect(page, tile, g.grid, g.gap);
  const view: TileView = {
    id: tile.id,
    rect,
    action: tile.action.type,
    dims: ['launch', 'open', 'command', 'widget'].includes(tile.action.type),
    texts: [],
    image: null,
    bar: null,
  };
  const { color, media, widget } = THEME;
  if (tile.action.type === 'widget') {
    const sample = options.widget?.(tile) ?? null;
    const opacity = sample?.stale ? THEME.staleOpa : 255;
    // draw_value and ui_widget_build: one line each, cut with dots. A value that fits keeps its content width.
    const maxW = rect.w - 2 * media.labelInset;
    const shown = sample?.value || '--';
    const fits = textSize(codepoints(shown), FONTS[48], COORD_MAX).w <= maxW;
    const valueBox = fits ? {} : { width: maxW, align: 'center' as const };
    const value = layoutLabel(shown, FONTS[48], { ...valueBox, height: FONTS[48].lineHeight, dots: true });
    view.texts.push(text(value, 48, color.text, centre(rect.w, rect.h, value.width, value.height, 0, -widget.valueDy), opacity));
    const caption = layoutLabel(sample?.caption || tile.label, FONTS[28], { width: maxW, height: FONTS[28].lineHeight, align: 'center', dots: true });
    view.texts.push(text(caption, 28, color.caption, centre(rect.w, rect.h, caption.width, caption.height, 0, widget.captionDy), opacity));
    const barW = !sample || sample.bar < 0 ? 0 : Math.trunc(Math.min(sample.bar, 1) * rect.w + 0.5);
    if (barW > 0) view.bar = { x: 0, y: rect.h - widget.barBottom - widget.barH, w: barW, h: widget.barH, radius: widget.barRadius };
    return view;
  }
  const glyph = tile.icon ? SYMBOLS[tile.icon] : undefined;
  const image = fileId(tile.icon);
  const stacked = glyph !== undefined || (image !== null && (options.hasImage?.(image) ?? true));
  if (!stacked) {
    const fitted = fitLabel(tile.label, rect.w, rect.h);
    view.texts.push(text(fitted.label, fitted.size, color.text, centre(rect.w, rect.h, fitted.label.width, fitted.label.height)));
    return view;
  }
  // stack_icon: the icon above, the label below it in the caption font, clipped to one line.
  const captionH = FONTS[28].lineHeight;
  const iconH = glyph !== undefined ? FONTS[48].lineHeight : 64;
  const caption = layoutLabel(tile.label, FONTS[28], { width: rect.w - 2 * media.labelInset, height: captionH, align: 'center' });
  view.texts.push(text(caption, 28, color.caption, centre(rect.w, rect.h, caption.width, caption.height, 0, div(iconH + media.gap, 2))));
  const lift = div(-(captionH + media.gap), 2);
  if (glyph !== undefined) {
    const icon = layoutLabel(glyph, FONTS[48]);
    view.texts.push(text(icon, 48, color.text, centre(rect.w, rect.h, icon.width, icon.height, 0, lift)));
  } else if (image !== null) {
    view.image = { id: image, ...centre(rect.w, rect.h, 64, 64, 0, lift), size: 64 };
  }
  return view;
}

function buildRows(g: Geometry, rows: RowSample[]): RowView[] {
  const { dash, color } = THEME;
  const rowH = div(g.grid.h - (dash.rowsMax - 1) * dash.rowGap, dash.rowsMax);
  const barMax = g.grid.w - g.dashBarX - dash.pad;
  // row_label: wrapped in its column for as many whole lines as the row holds, the last ending in dots when cut.
  const maxH = div(rowH, FONTS[28].lineHeight) * FONTS[28].lineHeight;
  const cell = (value: string, x: number, width: number, tint: string, align: Align = 'left') => {
    const height = Math.min(textSize(codepoints(value), FONTS[28], width).h, maxH);
    const label = layoutLabel(value, FONTS[28], { width, height, align, dots: true });
    return text(label, 28, tint, { x, y: div(rowH, 2) - div(label.height, 2) });
  };
  return rows.slice(0, dash.rowsMax).map((row, i) => {
    const barW = row.bar < 0 ? 0 : Math.trunc(Math.min(row.bar, 1) * barMax + 0.5);
    return {
      rect: { x: g.grid.x, y: g.grid.y + i * (rowH + dash.rowGap), w: g.grid.w, h: rowH },
      texts: [
        cell(row.label, dash.pad, g.dashValueX - 2 * dash.pad, color.caption),
        cell(row.value, g.dashValueX, g.dashBarX - g.dashValueX - dash.pad, color.text),
      ],
      bar: barW > 0 ? { x: g.dashBarX, y: div(rowH, 2) - div(dash.barH, 2), w: barW, h: dash.barH, radius: THEME.widget.barRadius } : null,
    };
  });
}

/** Everything the deck draws on one page, with the page's tiles in layout order. */
export function buildPage(page: Page, g: Geometry, options: SceneOptions = {}): PageView {
  if (page.kind === 'dashboard') {
    return { id: page.id, title: page.title, kind: page.kind, tiles: [], rows: buildRows(g, options.rows?.(page) ?? []) };
  }
  return { id: page.id, title: page.title, kind: page.kind, tiles: page.tiles.map((tile) => buildTile(page, tile, g, options)), rows: [] };
}

/** The page title at the top left, or null on a full-screen layout. */
export function buildTitle(title: string, g: Geometry): TextView | null {
  if (g.fullScreen) return null;
  return text(layoutLabel(title, FONTS[28]), 28, THEME.color.title, { x: g.titleX, y: g.titleY });
}

/** The page dots, the current one widened into a pill, or none on a full-screen layout. */
export function buildDots(g: Geometry, count: number, current: number): DotView[] {
  if (g.fullScreen) return [];
  return Array.from({ length: count }, (_, i) => {
    const active = i === current;
    const extra = active ? g.dotActiveExtra : 0;
    return {
      x: dotX(g, i, count) - div(g.dotSize, 2) - extra,
      y: g.dotY - div(g.dotSize, 2),
      w: g.dotSize + 2 * extra,
      h: g.dotSize,
      active,
    };
  });
}

/** Characters the deck's font has no glyph for. The deck draws each as an empty box. */
export function missingGlyphs(value: string): string[] {
  return [...new Set(Array.from(value).filter((c) => !FONTS[28].has(c.codePointAt(0)!)))];
}

/** Example values for widget tiles and dashboard pages, shaped like the CLIProxyAPI readings the app sends. */
const FIELD_SAMPLES: Record<string, Omit<WidgetSample, 'caption'>> = {
  five_hour_pct: { value: '42%', bar: 0.42 },
  seven_day_pct: { value: '61%', bar: 0.61 },
  five_hour_reset: { value: '2h 10m', bar: -1 },
  seven_day_reset: { value: '3d 4h', bar: -1 },
  plan: { value: 'pro', bar: -1 },
  status: { value: 'active', bar: -1 },
  success: { value: '128', bar: -1 },
  failed: { value: '3', bar: -1 },
  recent_10m: { value: '12', bar: 0.6 },
};

export function sampleWidget(tile: Tile): WidgetSample | null {
  if (tile.action.type !== 'widget') return null;
  const sample = FIELD_SAMPLES[tile.action.field];
  return sample ? { ...sample, caption: tile.label } : { value: '--', caption: tile.label, bar: -1 };
}

export function sampleRows(): RowSample[] {
  return [
    { label: 'codex alex', value: '5h 42% 7d 10%', bar: 0.42 },
    { label: 'claude sam', value: '5h 18% 7d 61%', bar: 0.61 },
    { label: 'gemini-cli jo', value: 'active', bar: 0.3 },
  ];
}
