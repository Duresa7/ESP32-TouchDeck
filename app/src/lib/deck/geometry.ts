// Where the deck draws, ported from firmware/components/td_ui_core/ui_geometry.c and the tile placement in
// ui_pages.c. Every value is an integer in screen pixels and uses the firmware's integer division, so the preview
// puts a tile on the same pixel as the deck.

export type Orientation = 'landscape' | 'portrait';

export interface Size {
  w: number;
  h: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Geometry {
  screen: Size;
  /** The strip that slides between pages. */
  band: Rect;
  /** Where a grid page's cells sit, inside the band. */
  grid: Rect;
  gap: number;
  /** Whether the layout hides the page title and dots and gives the tiles the whole screen. */
  fullScreen: boolean;
  titleX: number;
  titleY: number;
  dotY: number;
  dotPitch: number;
  dotSize: number;
  dotActiveExtra: number;
  bouncePx: number;
  slop: number;
  swipe: number;
  dashValueX: number;
  dashBarX: number;
}

/** The panel as it scans out, landscape. Portrait is the same glass turned 90 degrees clockwise. */
export const PANEL_W = 1024;
export const PANEL_H = 600;

/** The milestone 1 design was drawn for a short side of 600 px, and the dashboard columns for a 968 px grid. */
const DESIGN_SHORT = 600;
const DESIGN_DASH_W = 968;

/** C's integer division, which truncates toward zero. */
export const div = (a: number, b: number) => Math.trunc(a / b);

const scaled = (px: number, short: number) => div(px * short, DESIGN_SHORT);

export function screenSize(orientation: Orientation): Size {
  return orientation === 'portrait' ? { w: PANEL_H, h: PANEL_W } : { w: PANEL_W, h: PANEL_H };
}

/**
 * The deck's geometry for one screen. With `fullScreen` the deck draws no title and no dots, and the band and the grid
 * both cover the whole screen, with the same gap between tiles.
 */
export function geometry(screen: Size, fullScreen = false): Geometry {
  const s = Math.min(screen.w, screen.h);
  const titleStrip = scaled(56, s);
  const dotStrip = scaled(40, s);
  const marginX = scaled(28, s);
  const marginY = scaled(20, s);
  const band: Rect = fullScreen
    ? { x: 0, y: 0, w: screen.w, h: screen.h }
    : { x: 0, y: titleStrip, w: screen.w, h: screen.h - titleStrip - dotStrip };
  const grid: Rect = fullScreen
    ? { x: 0, y: 0, w: screen.w, h: screen.h }
    : { x: marginX, y: band.y + marginY, w: screen.w - 2 * marginX, h: band.h - 2 * marginY };
  return {
    screen,
    band,
    grid,
    gap: scaled(16, s),
    fullScreen,
    titleX: scaled(36, s),
    titleY: scaled(14, s),
    dotY: screen.h - scaled(20, s),
    dotPitch: scaled(34, s),
    dotSize: scaled(12, s),
    dotActiveExtra: scaled(11, s),
    bouncePx: scaled(48, s),
    slop: scaled(16, s),
    swipe: scaled(96, s),
    dashValueX: div(grid.w * 360, DESIGN_DASH_W),
    dashBarX: div(grid.w * 600, DESIGN_DASH_W),
  };
}

/** The x of the centre of dot `index` when `count` dots sit in a row centred on the screen. */
export function dotX(g: Geometry, index: number, count: number): number {
  const span = (count > 0 ? count - 1 : 0) * g.dotPitch;
  return div(g.screen.w - span, 2) + index * g.dotPitch;
}

export interface Cells {
  rows: number;
  cols: number;
}

export interface Span {
  row: number;
  col: number;
  rowSpan: number;
  colSpan: number;
}

const GRID_MAX = 8;
const sizeOk = (n: number) => Number.isInteger(n) && n >= 1 && n <= GRID_MAX;

/** Whether a tile has a span of at least 1 and stays inside a valid grid, as ui_pages.c's tile_fits. */
export function tileFits(page: Cells, tile: Span): boolean {
  return (
    sizeOk(page.rows) &&
    sizeOk(page.cols) &&
    tile.rowSpan >= 1 &&
    tile.colSpan >= 1 &&
    tile.row >= 0 &&
    tile.col >= 0 &&
    tile.row + tile.rowSpan <= page.rows &&
    tile.col + tile.colSpan <= page.cols
  );
}

/** The n-th cell edge from the area's start, counting each cell with the gap that follows it. */
const cellEdge = (length: number, gap: number, cells: number, n: number) => div(n * (length + gap), cells);

/**
 * Where a tile sits when the page's grid fills `area`, with `gap` pixels between neighbouring cells, as
 * ui_tile_rect. An empty rect for a tile that does not fit.
 */
export function tileRect(page: Cells, tile: Span, area: Rect, gap: number): Rect {
  if (!tileFits(page, tile)) return { x: 0, y: 0, w: 0, h: 0 };
  const x0 = cellEdge(area.w, gap, page.cols, tile.col);
  const x1 = cellEdge(area.w, gap, page.cols, tile.col + tile.colSpan) - gap;
  const y0 = cellEdge(area.h, gap, page.rows, tile.row);
  const y1 = cellEdge(area.h, gap, page.rows, tile.row + tile.rowSpan) - gap;
  return { x: area.x + x0, y: area.y + y0, w: x1 - x0, h: y1 - y0 };
}

/** The cell under a point in screen pixels, unclamped, so a point past the grid's edge names a cell outside it. */
export function cellAt(page: Cells, area: Rect, gap: number, x: number, y: number): [number, number] {
  const col = Math.floor(((x - area.x) * page.cols) / (area.w + gap));
  const row = Math.floor(((y - area.y) * page.rows) / (area.h + gap));
  return [row, col];
}
