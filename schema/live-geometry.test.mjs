import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const read = (name) => JSON.parse(readFileSync(new URL(`./live/${name}.json`, import.meta.url), 'utf8'));
const inside = (r, x, y) => r !== null && x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;

const nowPlaying = read('now-playing-vectors');
for (const c of nowPlaying.cases) {
  test(`now-playing: ${c.name}`, () => {
    const k = nowPlaying.constants;
    const size = Math.min(c.h - 2 * k.pad, Math.floor((c.w - 2 * k.pad) * 2 / 5), k.art_max);
    const art = size < k.art_min ? null : { x: k.pad, y: Math.floor((c.h - size) / 2), size };
    const x = art ? art.x + art.size + k.pad : k.pad;
    const text = { x, w: c.w - k.pad - x };
    const controls = c.h >= k.row_min_tile_h && text.w >= k.row_min_text_w;
    const width = Math.floor(text.w / 3);
    const parts = controls
      ? ['previous', 'play_pause', 'next'].map((part, i) => ({
          part, x: x + i * width, y: c.h - k.pad - k.row_h,
          w: i === 2 ? text.w - 2 * width : width, h: k.row_h,
        }))
      : [{ part: 'play_pause', x: 0, y: 0, w: c.w, h: c.h }];
    assert.deepEqual({ art, text, controls, parts }, { art: c.art, text: c.text, controls: c.controls, parts: c.parts });
    for (const hit of c.hits) {
      const part = hit.state === 0 ? null : parts.find((r) => inside(r, hit.x, hit.y))?.part ?? null;
      assert.equal(part, hit.part, JSON.stringify(hit));
    }
  });
}

test('bar zones and optional mute agree with the shared contract', () => {
  const bar = read('bar-vectors');
  assert.deepEqual(read('renderers').renderers.bar.parts_when, { mute: 'muted declared' });
  for (const c of bar.cases) {
    const k = bar.constants;
    const track = { x: k.pad, y: c.h - k.pad - k.track_h, w: c.w - 2 * k.pad, h: k.track_h };
    const top = Math.max(k.pad, track.y - k.capture_expand_y);
    const bottom = Math.min(c.h - k.pad, track.y + track.h + k.capture_expand_y);
    const capture = { x: track.x, y: top, w: track.w, h: bottom - top };
    const mute = c.muted_declared ? { x: k.pad, y: k.pad, w: k.glyph_size, h: k.glyph_size } : null;
    assert.deepEqual({ track, capture, mute }, { track: c.track, capture: c.capture, mute: c.mute }, c.name);
    for (const hit of c.hits) {
      const zone = !hit.fresh ? null : inside(mute, hit.x, hit.y) ? 'mute'
        : hit.input && inside(capture, hit.x, hit.y) ? 'value' : null;
      assert.equal(zone, hit.zone, `${c.name}: ${JSON.stringify(hit)}`);
    }
  }
});