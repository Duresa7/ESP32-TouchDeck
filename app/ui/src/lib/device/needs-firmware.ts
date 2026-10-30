import type { DeviceSummary } from '../../companion/api.ts';

const TILE_KINDS = ['static', 'gif', 'graph', 'bar', 'now-playing', 'photo-frame', 'clock'] as const;
export type TileKind = (typeof TILE_KINDS)[number];

const isTileKind = (value: unknown): value is TileKind => TILE_KINDS.some((kind) => kind === value);

export function tileKind(tile: object): TileKind {
  return 'render' in tile && isTileKind(tile.render) ? tile.render : 'static';
}

export function showsKind(device: Pick<DeviceSummary, 'capabilities'>, kind: TileKind): boolean {
  return device.capabilities ? device.capabilities.tile_kinds.includes(kind) : kind === 'static';
}

type Parts = [number, number, number];

function partsOf(version: string): Parts | null {
  const match = /^v?(\d+)\.(\d+)(?:\.(\d+))?/.exec(version.trim());
  return match ? [Number(match[1]), Number(match[2]), Number(match[3] ?? 0)] : null;
}

export function compareVersions(a: string, b: string): number | null {
  const left = partsOf(a);
  const right = partsOf(b);
  if (!left || !right) return null;
  for (let i = 0; i < 3; i++) if (left[i] !== right[i]) return left[i] - right[i];
  return 0;
}

export function newerFirmware(device: Pick<DeviceSummary, 'firmware'>, bundled: string | null): string | null {
  const running = device.firmware?.version;
  if (!running || !bundled) return null;
  return (compareVersions(bundled, running) ?? 0) > 0 ? bundled : null;
}

export function shortVersion(version: string): string {
  const parts = partsOf(version);
  return parts ? `${parts[0]}.${parts[1]}` : version;
}

export function needsFirmware(device: Pick<DeviceSummary, 'capabilities'>, kind: TileKind, offered: string | null): string | null {
  if (showsKind(device, kind)) return null;
  return offered ? `Needs Firmware ${shortVersion(offered)}` : 'Needs newer Firmware';
}

export function needsFirmwareField({ device, since }: { device: string; since: string | null }): string {
  return since ? `Needs Firmware ${shortVersion(since)}.` : `${device}'s Firmware can't change this yet.`;
}
