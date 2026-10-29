// The Firmware routes and types from #77, written by hand until the Companion ships them and api.gen.ts
// generates them. Delete this file then.
import type { DeviceId, Job } from './api.gen.ts';

export type BundleId = string;
export type BundleInfo = { id: BundleId, version: string, build: string, bytes: number, };
export type BundleRef = 'bundled' | BundleId;
export type FirmwareBuild = { version: string, build: string, };
export type PickResult = { bundle: BundleInfo | null, };
export type FirmwarePhase = 'checking' | 'sending' | 'rebooting' | 'confirming' | 'recovering';
export type FirmwareProgress = { phase: FirmwarePhase, pct?: number, };
export type FirmwareResult = { message: string, version: string, };

const firmwareEndpoints = {
  'firmware.bundled': { method: 'GET', path: '/firmware/bundled', access: 'session' },
  'firmware.pick': { method: 'POST', path: '/firmware/bundles/pick', access: 'session' },
  'firmware.update': { method: 'POST', path: '/devices/{device}/firmware/update', access: 'session' },
} as const;

export function createFirmwareRoutes(base: string, token: () => string) {
  async function call<T>(name: keyof typeof firmwareEndpoints, path: Record<string, string>, body: unknown): Promise<T> {
    const endpoint = firmwareEndpoints[name];
    let route: string = endpoint.path;
    for (const [key, value] of Object.entries(path)) route = route.replace(`{${key}}`, encodeURIComponent(value));
    const response = await fetch(new URL(`/api/v1${route}`, base), {
      method: endpoint.method,
      headers: { Authorization: `Bearer ${token()}`, ...(body == null ? {} : { 'Content-Type': 'application/json' }) },
      ...(body == null ? {} : { body: JSON.stringify(body) }),
    });
    if (!response.ok) throw (await response.json()).error;
    return response.json();
  }
  return {
    bundled: (_path: Record<string, never> = {}, _query: null = null, _body: null = null) => call<FirmwareBuild>('firmware.bundled', {}, null),
    pick: (_path: Record<string, never>, _query: null, body: { device: DeviceId, }) => call<PickResult>('firmware.pick', {}, body),
    update: (path: { device: DeviceId, }, _query: null, body: { bundle: BundleRef, }) => call<Job>('firmware.update', path, body),
  };
}

export type FirmwareRoutes = ReturnType<typeof createFirmwareRoutes>;
