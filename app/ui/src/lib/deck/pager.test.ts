import assert from 'node:assert/strict';
import { test } from 'vitest';
import { easeOut, goTo, move, nearestPage, pager, press, release, restX } from './pager.ts';

const make = (count = 3) => pager({ pageW: 1024, slop: 16, swipe: 96 }, count);

test('a press that stays inside the slop is a tap', () => {
  const p = make();
  press(p, 100, 100);
  assert.equal(move(p, 116, 84).press, 'tap');
  assert.equal(release(p), true);
});

test('a sideways drag swipes to the next page the moment it reaches the swipe distance', () => {
  const p = make();
  press(p, 500, 300);
  assert.equal(move(p, 480, 300).press, 'drag');
  const m = move(p, 404, 300);
  assert.deepEqual(m, { press: 'swiped', from: 0, to: 1, changed: true, blocked: 0 });
  assert.equal(move(p, 100, 300).changed, false, 'the rest of the press does nothing');
  assert.equal(release(p), false);
  assert.equal(restX(p), -1024);
});

test('an up or down drag never swipes and never taps', () => {
  const p = make();
  press(p, 500, 300);
  assert.equal(move(p, 500, 340).press, 'moved');
  assert.equal(move(p, 300, 340).changed, false);
  assert.equal(release(p), false);
});

test('a swipe past either end bounces the way the finger went', () => {
  const p = make(2);
  press(p, 500, 300);
  assert.equal(move(p, 600, 300).blocked, 1);
  goTo(p, 1);
  press(p, 500, 300);
  assert.equal(move(p, 400, 300).blocked, -1);
});

test('the title names the page nearest the middle, and the slide eases out like LVGL', () => {
  assert.equal(nearestPage(0, 1024, 3), 0);
  assert.equal(nearestPage(-511, 1024, 3), 0);
  assert.equal(nearestPage(-513, 1024, 3), 1);
  assert.equal(nearestPage(-4000, 1024, 3), 2);
  assert.equal(easeOut(0), 0);
  assert.equal(easeOut(1), 1);
  assert.ok(easeOut(0.25) > 0.5, 'most of the slide happens early');
});
