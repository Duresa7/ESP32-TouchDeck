import { fitText } from '../../deck/live-text.ts';
import { parseLayout, type Orientation, type Tile } from '../../editor/validate.ts';
import type { DeviceSummary } from '../api.ts';

/** One live value a mock Device holds, as `v2.live.set` last gave it. */
export interface FakeLiveSet {
  tile: string;
  stream: string;
  type: 'number' | 'text' | 'image';
  value: number | string | null;
}

export interface FakeLiveOptions {
  devices: ReadonlyMap<string, DeviceSummary>;
  stored(device: string, orientation: Orientation): string | null;
}

const json = (status: number, body: unknown) =>
  new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

const failure = (status: number, code: string, message: string) => json(status, { error: { code, message, details: null } });

const widgetOf = (tile: Tile) => (tile.action.type === 'widget' ? tile.action : null);

/** A mock Device's `v2.input` on a Tile's value stream: a move or end of a finite value. */
interface DeviceInput {
  tile: string;
  value: number;
}

function parseInput(parsed: unknown): DeviceInput | null {
  if (typeof parsed !== 'object' || parsed === null || !('tile' in parsed) || !('stream' in parsed) || !('phase' in parsed) || !('value' in parsed)) return null;
  const { tile, stream, phase, value } = parsed;
  if (typeof tile !== 'string' || stream !== 'value' || (phase !== 'move' && phase !== 'end') || typeof value !== 'number' || !Number.isFinite(value)) return null;
  return { tile, value };
}

async function readInput(request: Request): Promise<DeviceInput | null> {
  try {
    return parseInput(JSON.parse(await request.text()));
  } catch {
    return null;
  }
}

/** The fake Volume and Media live values on mock Devices, and the drags they send back, as docs/companion-api.md (Volume and Media) describes the UI fake. */
export function createFakeLive(options: FakeLiveOptions) {
  const { devices, stored } = options;
  const volume = { level: 0.64, muted: false };
  const track = { title: 'Night Drive', artist: 'Tidal Lines', state: 2 };

  function live(device: string): { tiles: Tile[]; kinds: readonly string[]; textBytes: number } | null {
    const summary = devices.get(device);
    const caps = summary?.capabilities;
    if (!summary || !caps || summary.link.state !== 'connected') return null;
    const text = stored(device, summary.orientation === 'portrait' ? 'portrait' : 'landscape');
    const parsed = text === null ? null : parseLayout(text);
    if (!parsed?.ok) return null;
    const tiles = parsed.layout.pages.flatMap((page) => (page.kind === 'grid' ? page.tiles : []));
    return { tiles, kinds: caps.tile_kinds, textBytes: caps.text_bytes };
  }

  function held(device: string): FakeLiveSet[] {
    const found = live(device);
    if (!found) return [];
    const sets: FakeLiveSet[] = [];
    for (const tile of found.tiles) {
      if (!found.kinds.includes(tile.render ?? 'static')) continue;
      const widget = widgetOf(tile);
      if (tile.render === 'bar' && widget?.source === 'volume' && widget.field === 'level') {
        sets.push({ tile: tile.id, stream: 'value', type: 'number', value: volume.level });
        if (tile.streams?.muted) sets.push({ tile: tile.id, stream: 'muted', type: 'number', value: volume.muted ? 1 : 0 });
      }
      if (tile.render === 'now-playing' && widget?.source === 'media') {
        const playing = track.state !== 0;
        sets.push(
          { tile: tile.id, stream: 'title', type: 'text', value: playing ? fitText(track.title, found.textBytes) : '' },
          { tile: tile.id, stream: 'artist', type: 'text', value: playing ? fitText(track.artist, found.textBytes) : '' },
          { tile: tile.id, stream: 'cover', type: 'image', value: null },
          { tile: tile.id, stream: 'state', type: 'number', value: track.state },
        );
      }
    }
    return sets;
  }

  function input(device: string, { tile: id, value }: DeviceInput): Response {
    const found = live(device);
    const tile = found?.kinds.includes('bar') ? found.tiles.find((t) => t.id === id) : undefined;
    const widget = tile ? widgetOf(tile) : null;
    if (!tile || tile.render !== 'bar' || tile.input !== true || widget?.source !== 'volume' || widget.field !== 'level') return failure(409, 'input.unbound', 'No bar Tile with input has that id.');
    volume.level = Math.min(Math.max(value, 0), 1);
    if (volume.level > 0) volume.muted = false;
    return json(204, null);
  }

  return {
    async test(request: Request, path: string): Promise<Response | null> {
      if (path === '/test/integrations/volume' && request.method === 'GET') return json(200, volume);
      const match = /^\/test\/devices\/([^/]+)\/(sets|input)$/.exec(path);
      if (!match) return null;
      const device = decodeURIComponent(match[1]);
      if (match[2] === 'sets') return request.method === 'GET' ? json(200, held(device)) : failure(405, 'route.method_not_allowed', 'Use GET.');
      if (request.method !== 'POST') return failure(405, 'route.method_not_allowed', 'Use POST.');
      const event = await readInput(request);
      return event ? input(device, event) : failure(400, 'request.invalid', "An input is a move or end of a finite value on a Tile's value stream.");
    },
  };
}
