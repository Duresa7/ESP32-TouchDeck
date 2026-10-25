import { mkdirSync, readFileSync, utimesSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { connectFirstDeck } from '../deck.ts';
import { note, shot } from '../report.ts';
import { DEV_JSON, FIXTURES, claudeDir, mockSets, openSources, saveJson, setChecked, until, useLayout, type Set } from '../sources.ts';

// TD_SOURCES_AGE_MINUTES=11 runs the stale half: the status file is that old when the app first reads it.
const age = Number(process.env.TD_SOURCES_AGE_MINUTES ?? 0);
// TD_SOURCES_PROBE=20 also writes the file that many times, 2.2 s apart, and records each write-to-set time.
const probe = Number(process.env.TD_SOURCES_PROBE ?? 0);
const FIXTURE = join(FIXTURES, 'statusline.json');
const SESSION = '8f3c1a52-fixture.json';

const CLAUDE_TILES = ['context', 'cost', 'model'];
const widgetSets = (sets: Set[], tile: string) => sets.filter((set) => set.m === 'widget.set' && set.tile === tile);

function writeStatus(dir: string, context?: number) {
  mkdirSync(dir, { recursive: true });
  const document = JSON.parse(readFileSync(FIXTURE, 'utf8'));
  if (context !== undefined) document.context_window.used_percentage = context;
  const path = join(dir, SESSION);
  const at = Date.now();
  writeFileSync(path, JSON.stringify(document));
  return { path, at };
}

describe(age > 0 ? `claude status file ${age} minutes old` : 'claude status tiles', () => {
  let dir = '';

  before(async () => {
    note('port', await connectFirstDeck());
    await openSources();
    dir = await claudeDir();
    note('claude_dir', dir);
    await setChecked('claude-enabled', true);
    await useLayout(DEV_JSON);
  });

  if (age > 0) {
    it('sends stale true for every Claude tile', async () => {
      const { path } = writeStatus(dir);
      const old = new Date(Date.now() - age * 60_000);
      utimesSync(path, old, old);
      const sets = await until(
        async () => {
          const all = await mockSets();
          return CLAUDE_TILES.every((tile) => widgetSets(all, tile).length > 0) && all;
        },
        5_000,
        'a widget.set for every Claude tile',
      );
      const stale = Object.fromEntries(CLAUDE_TILES.map((tile) => [tile, widgetSets(sets, tile).map((set) => set.stale)]));
      saveJson('stale.json', { age_minutes: age, stale, sets: sets.filter((set) => CLAUDE_TILES.includes(set.tile ?? '')) });
      note('stale', stale);
      for (const tile of CLAUDE_TILES) expect(stale[tile].every((value) => value === true)).toBe(true);
      await $('[data-testid="claude-state"][data-state="stale"]').waitForExist({ timeout: 5_000 });
      await shot('claude-stale');
    });
    return;
  }

  it('sends context, cost, and model with the fixture values within 2 seconds', async () => {
    const { at } = writeStatus(dir);
    const sets = await until(
      async () => {
        const all = await mockSets();
        return CLAUDE_TILES.every((tile) => widgetSets(all, tile).length > 0) && all;
      },
      2_000,
      'a widget.set for every Claude tile',
    );
    const latest = (tile: string) => widgetSets(sets, tile).at(-1)!;
    const took = Math.max(...CLAUDE_TILES.map((tile) => latest(tile).at_ms)) - at;
    note('file_to_set_ms', took);
    saveJson('widget-sets.json', { file_to_set_ms: took, sets: sets.filter((set) => CLAUDE_TILES.includes(set.tile ?? '')) });
    expect(latest('context')).toMatchObject({ value: '42%', caption: 'Context', bar: 0.42, stale: false });
    expect(latest('cost')).toMatchObject({ value: '$0.43', bar: -1, stale: false });
    expect(latest('model')).toMatchObject({ value: 'Opus', stale: false });
    expect(took).toBeLessThan(2_000);
    await $('[data-testid="claude-state"][data-state="live"]').waitForExist({ timeout: 5_000 });
    expect(await $('[data-testid="claude-value-context_pct"]').getText()).toBe('42%');
    expect(await $('[data-testid="claude-value-five_hour_pct"]').getText()).toBe('24%');
    await shot('claude-tiles');
  });

  it('sends one update for one change', async () => {
    const before = widgetSets(await mockSets(), 'context').length;
    writeStatus(dir, 50);
    await until(async () => (widgetSets(await mockSets(), 'context').at(-1)?.value === '50%') || undefined, 5_000, 'the 50% update');
    await browser.pause(2_500);
    const sets = widgetSets(await mockSets(), 'context');
    note('context_sets', sets.map((set) => ({ value: set.value, at_ms: set.at_ms })));
    expect(sets.length - before).toBe(1);
  });

  if (probe > 0) {
    it(`records the write-to-set time over ${probe} writes`, async function () {
      this.timeout(probe * 3_500 + 30_000);
      const times: number[] = [];
      for (let n = 0; n < probe; n++) {
        const context = 10 + (n % 80);
        const { at } = writeStatus(dir, context);
        const set = await until(
          async () => widgetSets(await mockSets(), 'context').find((s) => s.at_ms >= at && s.value === `${context}%`),
          6_000,
          `the set for write ${n}`,
        );
        times.push(set.at_ms - at);
        await browser.pause(2_200);
      }
      const sorted = [...times].sort((a, b) => a - b);
      const p95 = sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1)];
      note('probe', { times, p95 });
      saveJson('file-to-set.json', { times, p95 });
      expect(p95).toBeLessThanOrEqual(2_500);
    });
  }
});
