import assert from 'node:assert/strict';
import { describe, test } from 'vitest';
import bars from '../../../../../schema/live/bar-vectors.json';
import players from '../../../../../schema/live/now-playing-vectors.json';
import { BAR, barLayout, NOW_PLAYING, nowPlayingLayout } from './live-tiles.ts';

describe('now-playing geometry matches schema/live/now-playing-vectors.json', () => {
  test('the constants are the shared ones', () => {
    assert.deepEqual(NOW_PLAYING, players.constants);
  });

  for (const c of players.cases) {
    test(c.name, () => {
      const layout = nowPlayingLayout(c.w, c.h);
      assert.deepEqual(layout.art, c.art);
      assert.deepEqual(layout.text, c.text);
      assert.equal(layout.controls, c.controls);
      assert.deepEqual(layout.parts, c.parts);
    });
  }
});

describe('bar geometry matches schema/live/bar-vectors.json', () => {
  test('the constants are the shared ones', () => {
    assert.deepEqual(BAR, bars.constants);
  });

  for (const c of bars.cases) {
    test(c.name, () => {
      const layout = barLayout(c.w, c.h, c.muted_declared);
      assert.deepEqual(layout.track, c.track);
      assert.deepEqual(layout.mute, c.mute);
    });
  }
});
