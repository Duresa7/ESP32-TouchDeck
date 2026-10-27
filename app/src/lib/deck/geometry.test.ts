// The expected numbers are the firmware's own, from firmware/components/td_ui_core/test/test_ui_geometry.c.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { cellAt, dotX, geometry, screenSize, tileRect, type Orientation, type Rect } from './geometry.ts';

interface Tile {
  id: string;
  row: number;
  col: number;
  rowSpan: number;
  colSpan: number;
}

interface Page {
  id: string;
  rows: number;
  cols: number;
  tiles: Tile[];
}

const builtin = JSON.parse(readFileSync(join(import.meta.dirname, '..', '..', '..', '..', 'schema', 'examples', 'builtin.json'), 'utf8')) as {
  pages: Page[];
};

const geometryOf = (orientation: Orientation, fullScreen = false) => geometry(screenSize(orientation), fullScreen);
const overlap = (a: Rect, b: Rect) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

test('landscape geometry reproduces the milestone 1 pixel values', () => {
  const g = geometryOf('landscape');
  assert.deepEqual(g.screen, { w: 1024, h: 600 });
  assert.deepEqual(g.band, { x: 0, y: 56, w: 1024, h: 504 });
  assert.deepEqual(g.grid, { x: 28, y: 76, w: 968, h: 464 });
  assert.equal(g.gap, 16);
  assert.equal(g.titleX, 36);
  assert.equal(g.titleY, 14);
  assert.equal(g.dotY, 580);
  assert.equal(g.dotPitch, 34);
  assert.equal(g.dotSize, 12);
  assert.equal(g.dotActiveExtra, 11);
  assert.equal(g.bouncePx, 48);
  assert.equal(g.slop, 16);
  assert.equal(g.swipe, 96);
  assert.equal(g.dashValueX, 360);
  assert.equal(g.dashBarX, 600);
  assert.equal(dotX(g, 0, 2), 495);
  assert.equal(dotX(g, 1, 2), 529);
});

test('portrait geometry is 600 by 1024 with the band, grid and dots inside it', () => {
  const g = geometryOf('portrait');
  assert.deepEqual(g.screen, { w: 600, h: 1024 });
  assert.deepEqual(g.band, { x: 0, y: 56, w: 600, h: 928 });
  assert.deepEqual(g.grid, { x: 28, y: 76, w: 544, h: 888 });
  assert.equal(g.dotY, 1004);
  assert.equal(g.gap, 16);
  assert.equal(g.swipe, 96);
  assert.ok(g.dashValueX < g.dashBarX && g.dashBarX < g.grid.w);
  assert.equal(dotX(g, 0, 2), 283);
});

for (const orientation of ['landscape', 'portrait'] as const) {
  test(`every built-in tile fits the screen and overlaps no other tile in ${orientation}`, () => {
    const g = geometryOf(orientation);
    for (const page of builtin.pages) {
      const rects = page.tiles.map((tile) => tileRect(page, tile, g.grid, g.gap));
      rects.forEach((r, i) => {
        const id = page.tiles[i].id;
        assert.ok(r.w > 0 && r.h > 0, id);
        assert.ok(r.x >= g.grid.x && r.y >= g.grid.y, id);
        assert.ok(r.x + r.w <= g.grid.x + g.grid.w && r.y + r.h <= g.grid.y + g.grid.h, id);
        assert.ok(r.x + r.w <= g.screen.w && r.y + r.h <= g.screen.h, id);
        for (const other of rects.slice(0, i)) assert.ok(!overlap(r, other), id);
      });
    }
  });
}

test('portrait lays the F-keys grid out 4 wide and 4 tall, taller than wide', () => {
  const g = geometryOf('portrait');
  const page = builtin.pages[0];
  assert.equal(page.rows, 4);
  assert.equal(page.cols, 4);
  const find = (id: string) => page.tiles.find((tile) => tile.id === id)!;
  const f1 = tileRect(page, find('f1'), g.grid, g.gap);
  const win = tileRect(page, find('win'), g.grid, g.gap);
  assert.deepEqual(f1, { x: 28, y: 76, w: 124, h: 210 });
  assert.equal(win.x + win.w, g.grid.x + g.grid.w);
  assert.equal(win.y + win.h, g.grid.y + g.grid.h);
});

test('a tile that does not fit its grid has an empty rect', () => {
  const g = geometryOf('landscape');
  assert.deepEqual(tileRect({ rows: 2, cols: 2 }, { row: 1, col: 1, rowSpan: 2, colSpan: 1 }, g.grid, g.gap), { x: 0, y: 0, w: 0, h: 0 });
  assert.deepEqual(tileRect({ rows: 9, cols: 2 }, { row: 0, col: 0, rowSpan: 1, colSpan: 1 }, g.grid, g.gap), { x: 0, y: 0, w: 0, h: 0 });
});

test('full screen drops the title and dot strips and gives the grid the whole screen with the same gap', () => {
  for (const orientation of ['landscape', 'portrait'] as const) {
    const normal = geometryOf(orientation);
    const full = geometryOf(orientation, true);
    const whole = { x: 0, y: 0, w: full.screen.w, h: full.screen.h };
    assert.equal(full.fullScreen, true);
    assert.deepEqual(full.band, whole);
    assert.deepEqual(full.grid, whole);
    assert.equal(full.gap, normal.gap);
  }
  // The full-screen values from the firmware branch that adds the flag: the dashboard columns follow the wider grid,
  // and the pager, title, and dot fields keep their defaults.
  const land = geometryOf('landscape', true);
  const port = geometryOf('portrait', true);
  assert.deepEqual([land.dashValueX, land.dashBarX], [380, 634]);
  assert.deepEqual([port.dashValueX, port.dashBarX], [223, 371]);
  for (const key of ['titleX', 'titleY', 'dotY', 'dotPitch', 'dotSize', 'dotActiveExtra', 'bouncePx', 'slop', 'swipe'] as const) {
    assert.equal(land[key], geometryOf('landscape')[key], key);
    assert.equal(port[key], geometryOf('portrait')[key], key);
  }
  const page = builtin.pages[0];
  const rect = (g: typeof land, id: string) => tileRect(page, page.tiles.find((tile) => tile.id === id)!, g.grid, g.gap);
  assert.deepEqual(rect(land, 'f1'), { x: 0, y: 0, w: 244, h: 138 });
  assert.deepEqual(rect(land, 'win'), { x: 780, y: 462, w: 244, h: 138 });
  assert.deepEqual(rect(port, 'f1'), { x: 0, y: 0, w: 138, h: 244 });
  assert.deepEqual(rect(port, 'win'), { x: 462, y: 780, w: 138, h: 244 });
});

test('a point maps to the cell drawn under it, gaps included with the cell before them', () => {
  const g = geometryOf('landscape');
  const page = { rows: 3, cols: 4 };
  const r = tileRect(page, { row: 1, col: 2, rowSpan: 1, colSpan: 1 }, g.grid, g.gap);
  assert.deepEqual(cellAt(page, g.grid, g.gap, r.x, r.y), [1, 2]);
  assert.deepEqual(cellAt(page, g.grid, g.gap, r.x + r.w - 1, r.y + r.h - 1), [1, 2]);
  assert.deepEqual(cellAt(page, g.grid, g.gap, g.grid.x - 1, g.grid.y - 1), [-1, -1]);
  assert.deepEqual(cellAt(page, g.grid, g.gap, g.grid.x + g.grid.w + g.gap, g.grid.y), [0, 4]);
});
