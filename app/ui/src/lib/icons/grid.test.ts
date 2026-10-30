import { describe, expect, it } from 'vitest';
import { moveSpot } from './grid.ts';

describe('moving through the picker grid', () => {
  const sizes = [3, 0, 10];
  it.each([
    [{ section: 0, index: 2 }, 'ArrowRight', { section: 2, index: 0 }],
    [{ section: 2, index: 0 }, 'ArrowLeft', { section: 0, index: 2 }],
    [{ section: 0, index: 1 }, 'ArrowDown', { section: 2, index: 1 }],
    [{ section: 2, index: 5 }, 'ArrowDown', { section: 2, index: 9 }],
    [{ section: 2, index: 6 }, 'ArrowDown', { section: 2, index: 9 }],
    [{ section: 2, index: 9 }, 'ArrowDown', null],
    [{ section: 2, index: 3 }, 'ArrowUp', { section: 0, index: 2 }],
    [{ section: 0, index: 1 }, 'ArrowUp', 'search'],
    [{ section: 2, index: 4 }, 'Home', { section: 0, index: 0 }],
    [{ section: 0, index: 0 }, 'End', { section: 2, index: 9 }],
    [{ section: 0, index: 0 }, 'a', null],
  ] as const)('from %o, %s goes to %o', (at, key, to) => {
    expect(moveSpot(sizes, at, key, 4)).toEqual(to);
  });
});
