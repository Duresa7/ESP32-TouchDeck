import { expect, test, vi } from 'vitest';
import type { Layout, Tile } from '../../layout/model.ts';
import type { ControlLayout, DeviceSummary, LinkState } from '../api.ts';
import { hasControls, type Settable } from '../fake.ts';
import { createFakeCompanion } from '../fake.ts';
import { GIF_CAPS, sampleDevice } from '../samples.ts';

const CONNECTED: LinkState = { state: 'connected', port: 'COM3', since_ms: 0, protocol: 2 };
const LOST: LinkState = { state: 'lost', reason: 'unplugged', detail: '', at_ms: 1 };
const NO_COPY: ControlLayout = { state: 'no-pushed-copy', orientation: 'landscape', has_controls: true };

const widget = (source: string, field: string) => ({ type: 'widget' as const, source, field, account: '' });
const place = { row: 0, col: 0, rowSpan: 1, colSpan: 1, icon: null };
const bar = (source: string, field: string, muted = false): Tile => ({
  ...place, id: `${source}_${field}`, label: 'Bar', render: 'bar', input: true, streams: muted ? { value: 'number', muted: 'number' } : { value: 'number' }, action: widget(source, field),
});
const nowPlaying = (source: string): Tile => ({
  ...place, id: 'playing', label: 'Now playing', render: 'now-playing', streams: { title: 'text', artist: 'text', cover: 'image', state: 'number' }, action: widget(source, 'title'),
});
const graph: Tile = { ...place, id: 'cpu', label: 'CPU', render: 'graph', streams: { value: 'series' }, action: widget('this_pc', 'cpu') };
const layoutOf = (...tiles: Tile[]): Layout => ({ schema: 2, name: 'Test', orientation: 'landscape', pages: [{ kind: 'grid', id: 'main', title: 'Main', rows: 2, cols: 4, tiles }] });

const settable: Settable = ({ integration, stream }) => integration === 'volume' && stream === 'level';

test('has_controls counts renderer parts and settable bar streams, the way the Companion does', () => {
  expect(hasControls(layoutOf(nowPlaying('media')), settable)).toBe(true);
  expect(hasControls(layoutOf(bar('volume', 'level')), settable)).toBe(true);
  expect(hasControls(layoutOf(bar('forged', 'level', true)), settable)).toBe(true);
  expect(hasControls(layoutOf(bar('forged', 'level')), settable)).toBe(false);
  expect(hasControls(layoutOf(bar('volume', 'other')), settable)).toBe(false);
  expect(hasControls(layoutOf(nowPlaying('')), settable)).toBe(false);
  expect(hasControls(layoutOf(graph), settable)).toBe(false);
});

async function watch(device: DeviceSummary, layout: Layout) {
  const fake = createFakeCompanion({ devices: [device], layouts: { [device.id]: layout }, jobMs: 5 });
  const token = await fake.bridge.session();
  const seen: unknown[] = [];
  const socket = fake.openSocket('ws://127.0.0.1/api/v1/events', ['touchdeck.v1', `touchdeck.bearer.${token}`]);
  socket.onmessage = ({ data }) => {
    const frame: unknown = JSON.parse(String(data));
    if (typeof frame !== 'object' || frame === null || !('type' in frame) || frame.type !== 'device.updated' || !('data' in frame)) return;
    const summary = frame.data;
    if (typeof summary === 'object' && summary !== null && 'control_layout' in summary) seen.push(summary.control_layout);
  };
  const push = async (body: Layout) => {
    const sent = await fake.fetch(`http://127.0.0.1/api/v1/devices/${device.id}/layouts/landscape`, { method: 'PUT', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    expect(sent.status).toBe(202);
  };
  return { fake, seen, push };
}

test('a link change resets the assessment in the same update, then the Device reads as offline, unsupported or assessed', async () => {
  const media = layoutOf(bar('volume', 'level', true));
  const { fake, seen } = await watch({ ...sampleDevice('mock0001', 'Desk', CONNECTED), capabilities: GIF_CAPS, control_layout: NO_COPY }, media);

  fake.update('mock0001', { link: LOST });
  await vi.waitFor(() => expect(seen).toEqual([{ state: 'unavailable', reason: 'checking' }, { state: 'unavailable', reason: 'offline' }]));

  fake.update('mock0001', { link: CONNECTED });
  await vi.waitFor(() => expect(seen.slice(2)).toEqual([{ state: 'unavailable', reason: 'checking' }, NO_COPY]));

  seen.length = 0;
  fake.update('mock0001', { link: { ...CONNECTED, since_ms: 1 }, capabilities: null });
  await vi.waitFor(() => expect(seen).toEqual([{ state: 'unavailable', reason: 'checking' }, { state: 'unavailable', reason: 'unsupported' }]));
});

test('a push the Device takes goes from checking to matches, and one it refuses leaves the assessment alone', async () => {
  const media = layoutOf(bar('volume', 'level', true));
  const { seen, push } = await watch({ ...sampleDevice('mock0001', 'Desk', CONNECTED), capabilities: { ...GIF_CAPS, anim: undefined }, control_layout: NO_COPY }, media);

  await push(media);
  await vi.waitFor(() => expect(seen).toEqual([{ state: 'unavailable', reason: 'checking' }, { ...NO_COPY, state: 'matches' }]));

  seen.length = 0;
  const gif: Tile = { ...place, id: 'gif', label: 'GIF', render: 'gif', icon: null, asset: `sha256:${'1'.repeat(64)}`, action: { type: 'none' } };
  await push(layoutOf(bar('volume', 'level', true), { ...gif, col: 2 }));
  await new Promise((done) => setTimeout(done, 50));
  expect(seen).toEqual([]);
});
