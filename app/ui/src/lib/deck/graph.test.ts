import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'vitest';
import { graphCeiling, graphPoints, graphRange } from './graph.ts';

interface GraphVector {
  name: string;
  cap: number;
  w: number;
  h: number;
  samples: number[];
  range?: [number, number];
  points: [number, number][];
}

const vectors = JSON.parse(
  readFileSync(join(import.meta.dirname, '..', '..', '..', '..', '..', 'schema', 'live', 'graph-vectors.json'), 'utf8'),
) as { cases: GraphVector[] };

test('the ceiling is 1, 2 or 5 times a power of ten, as the Firmware rounds it', () => {
  assert.deepEqual([1, 1.01, 37, 99.9, 0.37].map(graphCeiling), [1, 2, 50, 100, 0.5]);
});

test('every shared vector places its points as the Firmware does', () => {
  assert.ok(vectors.cases.length >= 10);
  for (const c of vectors.cases) {
    const points = graphPoints(c.samples, c.cap, c.w, c.h).map(({ x, y }) => [x, y]);
    assert.deepEqual(points, c.points, c.name);
    if (c.range) {
      const { lo, hi } = graphRange(c.samples.slice(-c.cap));
      assert.deepEqual([lo, hi], c.range, c.name);
    }
  }
});
