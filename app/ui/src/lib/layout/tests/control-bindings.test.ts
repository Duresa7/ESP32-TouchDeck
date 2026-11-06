import { describe, expect, test } from 'vitest';
import { controlBindings, controlWords, type KnownIntegration } from '../controls.ts';
import type { Layout, Tile } from '../model.ts';

const INTEGRATIONS: KnownIntegration[] = [
  { id: 'volume', name: 'Volume', streams: [{ id: 'level', kind: 'numeric', unit: '%', settable: true }, { id: 'muted', kind: 'numeric', unit: null, settable: false }] },
  { id: 'media', name: 'Media', streams: [{ id: 'title', kind: 'text', unit: null, settable: false }] },
  { id: 'this_pc', name: 'This PC', streams: [{ id: 'cpu_pct', kind: 'numeric', unit: '%', settable: false }] },
];

const place = (id: string, row: number, col: number) => ({ id, label: id, row, col, rowSpan: 1, colSpan: 1 });
const widget = (source: string, field: string, account = '') => ({ type: 'widget' as const, source, field, account });

const volume: Tile = { ...place('volume', 1, 0), icon: null, render: 'bar', streams: { value: 'number', muted: 'number' }, input: true, action: widget('volume', 'level') };
const level: Tile = { ...place('level', 1, 1), icon: null, render: 'bar', streams: { value: 'number' }, action: widget('volume', 'level') };
const playing: Tile = { ...place('playing', 0, 2), icon: null, render: 'now-playing', streams: { title: 'text', artist: 'text', cover: 'image', state: 'number' }, action: widget('media', 'title') };
const cpu: Tile = { ...place('cpu', 0, 0), icon: null, render: 'bar', streams: { value: 'number' }, input: true, action: widget('this_pc', 'cpu_pct') };
const forged: Tile = { ...place('forged', 0, 1), icon: null, render: 'bar', streams: { value: 'number' }, input: true, action: widget('forged', 'level', 'other PC') };
const stranger: Tile = { ...place('stranger', 0, 3), icon: null, render: 'now-playing', streams: { title: 'text', artist: 'text', cover: 'image', state: 'number' }, action: widget('media', 'title', 'Work') };
const unbound: Tile = { ...place('unbound', 1, 2), icon: null, render: 'now-playing', streams: { title: 'text', artist: 'text', cover: 'image', state: 'number' }, action: widget('', 'title') };
const graph: Tile = { ...place('graph', 1, 3), icon: null, render: 'graph', streams: { value: 'number' }, action: widget('this_pc', 'cpu_pct') };
const key: Tile = { ...place('key', 0, 0), icon: null, action: { type: 'key', usage: 4, mods: [] } };

const layout: Layout = {
  schema: 2,
  name: 'Desk',
  orientation: 'landscape',
  pages: [
    { kind: 'grid', id: 'media', title: 'Media', rows: 2, cols: 4, tiles: [level, volume, playing, unbound] },
    { kind: 'dashboard', id: 'usage', title: 'Usage', source: 'cliproxyapi' },
    { kind: 'grid', id: 'more', title: 'More', rows: 2, cols: 4, tiles: [graph, stranger, forged, cpu, { ...key, row: 1, col: 0 }] },
  ],
};

describe('the control bindings of a Layout', () => {
  test('lists every Tile that controls an Integration on every Page, in place order, with names from this PC', () => {
    const bindings = controlBindings(layout, INTEGRATIONS);
    expect(bindings.map((b) => `${b.page.title}/${b.tile.label}`)).toEqual(['Media/playing', 'Media/volume', 'More/forged', 'More/stranger']);
    expect(bindings[0]).toEqual({
      page: { id: 'media', title: 'Media' },
      tile: { id: 'playing', label: 'playing' },
      integration: { kind: 'known', id: 'media', name: 'Media' },
      account: { kind: 'default' },
      controls: ['previous', 'play_pause', 'next'],
    });
    expect(bindings[1]).toMatchObject({ integration: { kind: 'known', name: 'Volume' }, controls: ['drag', 'mute'] });
  });

  test('names an Integration or account this PC does not know as unknown, not by the Tile label', () => {
    const [, , forgedBinding, strangerBinding] = controlBindings(layout, INTEGRATIONS);
    expect(forgedBinding).toMatchObject({ tile: { label: 'forged' }, integration: { kind: 'unknown', id: 'forged' }, account: { kind: 'unknown', id: 'other PC' }, controls: ['drag'] });
    expect(strangerBinding).toMatchObject({ integration: { kind: 'known', name: 'Media' }, account: { kind: 'unknown', id: 'Work' } });
    expect(controlBindings(layout, []).map((b) => b.integration.kind)).toEqual(['unknown', 'unknown', 'unknown', 'unknown', 'unknown']);
  });

  test('leaves out read-only bars, graphs, plain Tiles, Tiles without an Integration and drags on a stream that cannot be set', () => {
    const ids = controlBindings(layout, INTEGRATIONS).map((b) => b.tile.id);
    for (const id of ['level', 'graph', 'key', 'unbound', 'cpu']) expect(ids).not.toContain(id);
    expect(controlBindings({ ...layout, pages: [layout.pages[1]] }, INTEGRATIONS)).toEqual([]);
  });

  test('words each control the way the editor names it', () => {
    expect(controlWords(['drag', 'mute'])).toBe('Drag to change · Mute');
    expect(controlWords(['previous', 'play_pause', 'next'])).toBe('Previous track · Play or pause · Next track');
  });
});
