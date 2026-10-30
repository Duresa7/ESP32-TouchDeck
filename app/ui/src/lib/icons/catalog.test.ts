import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CatalogHit } from '../../companion/api.ts';
import { RECORDED_HITS } from '../../companion/fake-catalog.data.ts';
import { catalogSource, catalogTint, createCatalogSearch, monogram, SEARCH_DELAY_MS, type SearchState } from './catalog.ts';

function recorded(id: string) {
  const hit = RECORDED_HITS.find((h) => h.source === 'simple-icons' && h.id === id);
  if (!hit) throw new Error(`no recorded Simple Icons hit ${id}`);
  return hit;
}

const spotify = recorded('spotify');

function harness(answer: (query: string) => readonly CatalogHit[] = () => [spotify]) {
  const asked: { query: string; signal: AbortSignal }[] = [];
  const states: SearchState[] = [];
  const search = createCatalogSearch({
    search: (query, signal) => {
      asked.push({ query, signal });
      return Promise.resolve(answer(query));
    },
    onChange: (state) => states.push(state),
  });
  return { search, asked, states };
}

describe('catalog search', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('sends one request for fast typing, after the debounce', async () => {
    const { search, asked, states } = harness();
    for (const typed of ['s', 'sp', 'spo', 'spot', 'spoti', 'spotif', 'spotify']) {
      search.type(typed);
      await vi.advanceTimersByTimeAsync(40);
    }
    expect(asked).toEqual([]);
    await vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS);
    expect(asked.map((a) => a.query)).toEqual(['spotify']);
    expect(states.at(-1)).toEqual({ kind: 'done', query: 'spotify', hits: [spotify] });
  });

  it('starts no timer until typed into, and leaves none after close', async () => {
    const { search, asked } = harness(() => []);
    expect(vi.getTimerCount()).toBe(0);
    search.type('plex');
    expect(vi.getTimerCount()).toBe(1);
    search.close();
    expect(vi.getTimerCount()).toBe(0);
    search.type('obs');
    await vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS * 2);
    expect(asked).toEqual([]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('aborts the search in flight on close and drops its answer', async () => {
    let finish: (hits: readonly CatalogHit[]) => void = () => {};
    const states: SearchState[] = [];
    const signals: AbortSignal[] = [];
    const search = createCatalogSearch({
      search: (_, signal) => {
        signals.push(signal);
        return new Promise((resolve) => (finish = resolve));
      },
      onChange: (state) => states.push(state),
    });
    search.type('git');
    await vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS);
    expect(signals[0].aborted).toBe(false);
    search.close();
    expect(signals[0].aborted).toBe(true);
    finish([spotify]);
    await vi.advanceTimersByTimeAsync(0);
    expect(states.map((s) => s.kind)).toEqual(['loading']);
  });

  it('clearing the query goes back to idle without a request', async () => {
    const { search, asked, states } = harness();
    search.type('obs');
    search.type('   ');
    await vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS * 2);
    expect(asked).toEqual([]);
    expect(states.at(-1)).toEqual({ kind: 'idle' });
  });
});

describe('catalog picks', () => {
  it('tints Simple Icons with the brand color when it shows on a Tile, otherwise with screen ink', () => {
    expect(catalogTint({ source: 'simple-icons', hex: '1ED760' })).toEqual([0x1e, 0xd7, 0x60]);
    expect(catalogTint({ source: 'simple-icons', hex: '000000' })).toEqual([0xf0, 0xf0, 0xf8]);
    expect(catalogTint({ source: 'simple-icons', hex: null })).toEqual([0xf0, 0xf0, 0xf8]);
    expect(catalogTint({ source: 'selfhst', hex: null })).toBeNull();
  });

  it('builds a catalog source from a hit', () => {
    expect(catalogSource(spotify)).toEqual({ kind: 'catalog', catalog: 'simple-icons', id: 'spotify', variant: 'base', padding: 0, background: null, tint: [0x1e, 0xd7, 0x60] });
  });

  it.each([
    ['Spotify', 'S'],
    ['.NET', 'N'],
    ['/e/', 'E'],
    ['1&1', '1'],
    ['ŠKODA', 'Š'],
    ['大众点评', '大'],
  ])('marks %s with %s', (name, mark) => {
    expect(monogram(name)).toBe(mark);
  });
});
