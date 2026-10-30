import { describe, expect, it } from 'vitest';
import { forget, readRecent, RECENT_KEY, RECENT_MAX, recentKey, remember, writeRecent, type RecentIcon } from './recent.ts';

const asset = (n: number): RecentIcon => ({ kind: 'asset', asset: n.toString(16).padStart(64, '0'), name: `Logo ${n}` });

function memory() {
  const items = new Map<string, string>();
  return { getItem: (key: string) => items.get(key) ?? null, setItem: (key: string, value: string) => void items.set(key, value) };
}

describe('recent picks', () => {
  it('keeps the newest first and moves a repeat pick to the front', () => {
    let list: RecentIcon[] = [];
    for (const n of [1, 2, 3]) list = remember(list, asset(n));
    list = remember(list, asset(1));
    expect(list.map((icon) => icon.name)).toEqual(['Logo 1', 'Logo 3', 'Logo 2']);
  });

  it(`keeps at most ${RECENT_MAX}`, () => {
    let list: RecentIcon[] = [];
    for (let n = 1; n <= 20; n++) list = remember(list, asset(n));
    expect(list).toHaveLength(RECENT_MAX);
    expect(list[0].name).toBe('Logo 20');
    expect(list.at(-1)?.name).toBe('Logo 9');
  });

  it('forgets one that no longer resolves', () => {
    const list = [asset(1), { kind: 'symbol', icon: 'lv:mute', name: 'Speaker off' } satisfies RecentIcon, asset(2)];
    expect(forget(list, recentKey(asset(1))).map((icon) => icon.name)).toEqual(['Speaker off', 'Logo 2']);
  });

  it('round-trips through storage and ignores entries it cannot read', () => {
    const storage = memory();
    writeRecent(storage, [asset(1), { kind: 'file', id: '0123456789abcdef', name: 'Mail' }]);
    expect(readRecent(storage)).toEqual([asset(1), { kind: 'file', id: '0123456789abcdef', name: 'Mail' }]);
    storage.setItem(RECENT_KEY, JSON.stringify([{ kind: 'asset', asset: 'nope', name: 'Bad' }, { kind: 'symbol', icon: 'lv:next', name: 'Next' }, 'junk']));
    expect(readRecent(storage)).toEqual([{ kind: 'symbol', icon: 'lv:next', name: 'Next' }]);
    storage.setItem(RECENT_KEY, '{not json');
    expect(readRecent(storage)).toEqual([]);
    expect(readRecent(null)).toEqual([]);
  });
});
