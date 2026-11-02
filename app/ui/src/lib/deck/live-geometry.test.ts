import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { test } from 'vitest';
import vectors from '../../../../../schema/live/tile-geometry-vectors.json';
import { geometry, tileRect } from './geometry.ts';

for (const c of vectors.cases) {
  test(c.name, () => {
    const g = geometry(c.screen, c.fullScreen);
    assert.deepEqual(tileRect(c.grid, c.tile, g.grid, g.gap), c.app_rect);
    assert.equal(isDeepStrictEqual(c.app_rect, c.firmware_rect), c.agree);
  });
}
