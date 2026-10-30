export const RECENT_MAX = 12;
export const RECENT_KEY = 'touchdeck.recent-icons';

export type RecentIcon =
  | { kind: 'asset'; asset: string; name: string }
  | { kind: 'file'; id: string; name: string }
  | { kind: 'symbol'; icon: string; name: string };

type Storage = Pick<globalThis.Storage, 'getItem' | 'setItem'>;

const ASSET = /^[0-9a-f]{64}$/;
const FILE = /^[0-9a-f]{16}$/;
const SYMBOL = /^lv:[a-z_]+$/;

export function recentKey(icon: RecentIcon): string {
  switch (icon.kind) {
    case 'asset':
      return `asset:${icon.asset}`;
    case 'file':
      return `file:${icon.id}`;
    case 'symbol':
      return icon.icon;
    default: {
      const exhaustive: never = icon;
      return exhaustive;
    }
  }
}

export function remember(list: readonly RecentIcon[], icon: RecentIcon): RecentIcon[] {
  const key = recentKey(icon);
  return [icon, ...list.filter((other) => recentKey(other) !== key)].slice(0, RECENT_MAX);
}

export function forget(list: readonly RecentIcon[], key: string): RecentIcon[] {
  return list.filter((icon) => recentKey(icon) !== key);
}

function parseRecent(value: unknown): RecentIcon | null {
  if (typeof value !== 'object' || value === null || !('kind' in value) || !('name' in value) || typeof value.name !== 'string') return null;
  const name = value.name;
  if (value.kind === 'asset' && 'asset' in value && typeof value.asset === 'string' && ASSET.test(value.asset)) return { kind: 'asset', asset: value.asset, name };
  if (value.kind === 'file' && 'id' in value && typeof value.id === 'string' && FILE.test(value.id)) return { kind: 'file', id: value.id, name };
  if (value.kind === 'symbol' && 'icon' in value && typeof value.icon === 'string' && SYMBOL.test(value.icon)) return { kind: 'symbol', icon: value.icon, name };
  return null;
}

export function readRecent(storage: Storage | null): RecentIcon[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(storage?.getItem(RECENT_KEY) ?? '[]');
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  return parsed.flatMap((item) => parseRecent(item) ?? []).slice(0, RECENT_MAX);
}

export function writeRecent(storage: Storage | null, list: readonly RecentIcon[]): void {
  try {
    storage?.setItem(RECENT_KEY, JSON.stringify(list.slice(0, RECENT_MAX)));
  } catch {
    return;
  }
}
