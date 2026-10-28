// The deck's pager, ported from firmware/components/td_ui_core/ui_pager.c: which page shows and what a finger on the
// glass is doing to it. A page changes the moment the finger has travelled `swipe` pixels sideways, before the lift.
// A swipe past the first or last page changes nothing and reports the way the pages should bounce.

export type Press = 'none' | 'tap' | 'drag' | 'moved' | 'swiped';

export interface Pager {
  pageW: number;
  slop: number;
  swipe: number;
  count: number;
  index: number;
  press: Press;
  x0: number;
  y0: number;
}

export interface Move {
  press: Press;
  from: number;
  to: number;
  changed: boolean;
  /** -1 for a leftward swipe past the last page, 1 for a rightward one past the first, else 0. */
  blocked: -1 | 0 | 1;
}

export function pager(cfg: { pageW: number; slop: number; swipe: number }, count: number, index = 0): Pager {
  return { ...cfg, count: Math.max(1, count), index: Math.min(Math.max(0, index), Math.max(0, count - 1)), press: 'none', x0: 0, y0: 0 };
}

export function press(p: Pager, x: number, y: number) {
  p.press = 'tap';
  p.x0 = x;
  p.y0 = y;
}

export function move(p: Pager, x: number, y: number): Move {
  const from = p.index;
  const dx = Math.trunc(x) - p.x0;
  const dy = Math.trunc(y) - p.y0;
  const ax = Math.abs(dx);
  const ay = Math.abs(dy);
  let blocked: Move['blocked'] = 0;
  if (p.press === 'tap' && (ax > p.slop || ay > p.slop)) p.press = ax >= ay ? 'drag' : 'moved';
  if (p.press === 'drag' && ax >= p.swipe) {
    p.press = 'swiped';
    if (dx < 0 && p.index + 1 < p.count) p.index++;
    else if (dx > 0 && p.index > 0) p.index--;
    else blocked = dx < 0 ? -1 : 1;
  }
  return { press: p.press, from, to: p.index, changed: p.index !== from, blocked };
}

/** Ends the press. True when it was a tap. */
export function release(p: Pager): boolean {
  const tap = p.press === 'tap';
  p.press = 'none';
  return tap;
}

export const restX = (p: Pager) => -p.index * p.pageW;

export function goTo(p: Pager, index: number): boolean {
  if (index < 0 || index >= p.count) return false;
  p.index = index;
  p.press = 'none';
  return true;
}

/** LVGL's lv_anim_path_ease_out: a cubic Bezier through 0, 900/1024, 950/1024 and 1, in time. */
export function easeOut(t: number): number {
  const u = 1 - t;
  return 3 * u * u * t * (900 / 1024) + 3 * u * t * t * (950 / 1024) + t * t * t;
}

/** The page nearest the middle of the screen for a track at `x`, which the title names, as ui_deck.c's sync_title. */
export function nearestPage(x: number, pageW: number, count: number): number {
  const nearest = Math.trunc((pageW / 2 - x) / pageW);
  return Math.min(Math.max(nearest, 0), Math.max(0, count - 1));
}
