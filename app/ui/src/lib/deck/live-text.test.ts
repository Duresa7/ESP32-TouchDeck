import assert from 'node:assert/strict';
import { describe, test } from 'vitest';
import vectors from '../../../../../schema/live/text-vectors.json';
import { fitText } from './live-text.ts';

describe('live text matches schema/live/text-vectors.json', () => {
  for (const c of vectors.cases) {
    test(`${c.kind}: ${c.name}`, () => {
      assert.equal(fitText(c.input, c.max_bytes), c.output);
    });
  }

  test('the result never passes the byte limit', () => {
    for (const c of vectors.cases) assert.ok(new TextEncoder().encode(fitText(c.input, c.max_bytes)).length <= c.max_bytes, c.name);
  });
});
