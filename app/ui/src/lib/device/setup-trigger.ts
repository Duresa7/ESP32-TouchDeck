import type { DeviceSummary } from '../../companion/api.ts';

export function setupCandidate(devices: readonly DeviceSummary[], skipped: ReadonlySet<string>): DeviceSummary | null {
  return devices.find((d) => d.setup === 'needs-setup' && d.link.state === 'connected' && !skipped.has(d.id)) ?? null;
}

export const SKIPPED_KEY = 'touchdeck.setup-skipped';

export type SkipStorage = Pick<Storage, 'getItem' | 'setItem'>;

export function readSkipped(storage: SkipStorage | null): Set<string> {
  try {
    const value: unknown = JSON.parse(storage?.getItem(SKIPPED_KEY) ?? '[]');
    return new Set(Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : []);
  } catch {
    return new Set();
  }
}

export function writeSkipped(storage: SkipStorage | null, skipped: ReadonlySet<string>) {
  try {
    storage?.setItem(SKIPPED_KEY, JSON.stringify([...skipped]));
  } catch {}
}
