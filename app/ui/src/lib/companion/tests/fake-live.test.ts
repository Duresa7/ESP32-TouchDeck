import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from 'vitest';
import type { DeviceSummary } from '../api.ts';
import { createFakeLive } from '../fake.ts';
import { GIF_CAPS, sampleDevice } from '../samples.ts';

const media = readFileSync(join(import.meta.dirname, '..', '..', '..', '..', '..', '..', 'schema', 'examples', 'media.json'), 'utf8');
const CONNECTED = { state: 'connected', port: 'COM3', since_ms: 0, protocol: 2 } as const;

function live(kinds: string[]) {
  const device: DeviceSummary = { ...sampleDevice('mock0001', 'Desk', CONNECTED), capabilities: { ...GIF_CAPS, tile_kinds: kinds, text_bytes: 128 } };
  const fake = createFakeLive({ devices: new Map([[device.id, device]]), stored: () => media });
  const call = async (path: string, init?: RequestInit) => (await fake.test(new Request(`http://127.0.0.1${path}`, init), path))!;
  const send = (body: unknown) => call('/test/devices/mock0001/input', { method: 'POST', body: typeof body === 'string' ? body : JSON.stringify(body) });
  const level = async () => (await (await call('/test/integrations/volume')).json()) as { level: number; muted: boolean };
  const sets = async () => (await (await call('/test/devices/mock0001/sets')).json()) as unknown[];
  return { send, level, sets };
}

const move = (tile: string, value: unknown) => ({ page: 'media', tile, stream: 'value', phase: 'move', value });

test('a Device that draws bars and now-playing Tiles holds the fake Volume level and track', async () => {
  const { sets } = live(['static', 'bar', 'now-playing']);
  expect(await sets()).toEqual([
    { tile: 'volume', stream: 'value', type: 'number', value: 0.64 },
    { tile: 'volume', stream: 'muted', type: 'number', value: 0 },
    { tile: 'level', stream: 'value', type: 'number', value: 0.64 },
    { tile: 'playing', stream: 'title', type: 'text', value: 'Night Drive' },
    { tile: 'playing', stream: 'artist', type: 'text', value: 'Tidal Lines' },
    { tile: 'playing', stream: 'cover', type: 'image', value: null },
    { tile: 'playing', stream: 'state', type: 'number', value: 2 },
  ]);
  expect(await live(['static', 'graph']).sets()).toEqual([]);
});

test('a drag sets the level only through a bar Tile that takes input, clamped to 0..1', async () => {
  const { send, level } = live(['static', 'bar', 'now-playing']);
  expect((await send(move('volume', 0.25))).status).toBe(204);
  expect(await level()).toEqual({ level: 0.25, muted: false });
  expect((await send(move('volume', -3))).status).toBe(204);
  expect(await level()).toEqual({ level: 0, muted: false });
  expect((await send(move('volume', 9))).status).toBe(204);
  expect(await level()).toEqual({ level: 1, muted: false });
  for (const tile of ['level', 'playing', 'nope']) expect((await send(move(tile, 0.5))).status).toBe(409);
  for (const body of [move('volume', 'loud'), move('volume', null), { ...move('volume', 0.5), phase: 'start' }, { ...move('volume', 0.5), stream: 'muted' }, 'not json', [move('volume', 0.5)]]) {
    expect((await send(body)).status).toBe(400);
  }
  expect(await level()).toEqual({ level: 1, muted: false });
});

test('a Device that draws no bars takes no drag', async () => {
  const { send, level } = live(['static', 'graph']);
  expect((await send(move('volume', 0.2))).status).toBe(409);
  expect(await level()).toEqual({ level: 0.64, muted: false });
});
