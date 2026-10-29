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

export type Bounce = 'left' | 'right';

export interface Move {
  press: Press;
  from: number;
  to: number;
  changed: boolean;
  bounce: Bounce | null;
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
  let bounce: Bounce | null = null;
  if (p.press === 'tap' && (ax > p.slop || ay > p.slop)) p.press = ax >= ay ? 'drag' : 'moved';
  if (p.press === 'drag' && ax >= p.swipe) {
    p.press = 'swiped';
    if (dx < 0 && p.index + 1 < p.count) p.index++;
    else if (dx > 0 && p.index > 0) p.index--;
    else bounce = dx < 0 ? 'left' : 'right';
  }
  return { press: p.press, from, to: p.index, changed: p.index !== from, bounce };
}

export function release(p: Pager): Press {
  const ended = p.press;
  p.press = 'none';
  return ended;
}

export const restX = (p: Pager) => -p.index * p.pageW;

export function goTo(p: Pager, index: number): boolean {
  if (index < 0 || index >= p.count) return false;
  p.index = index;
  p.press = 'none';
  return true;
}

export function easeOut(t: number): number {
  const u = 1 - t;
  return 3 * u * u * t * (900 / 1024) + 3 * u * t * t * (950 / 1024) + t * t * t;
}

export function nearestPage(x: number, pageW: number, count: number): number {
  const nearest = Math.trunc((pageW / 2 - x) / pageW);
  return Math.min(Math.max(nearest, 0), Math.max(0, count - 1));
}
