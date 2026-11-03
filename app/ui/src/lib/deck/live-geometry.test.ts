import assert from 'node:assert/strict';
import { test } from 'vitest';
import vectors from '../../../../../schema/live/tile-geometry-vectors.json';
import { geometry, tileRect } from './geometry.ts';

for (const c of vectors.cases) {
  test(c.name, () => {
    const g = geometry(c.screen, c.fullScreen);
    assert.deepEqual(tileRect(c.grid, c.tile, g.grid, g.gap), c.rect);
  });
}
