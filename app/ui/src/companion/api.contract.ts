// Hand-written from docs/companion-api.md until the Companion generates api.gen.ts (#71).
// It has the same exports and shapes as the generated file, so api.ts can switch to it
// without touching a call site. Delete this file when api.ts re-exports api.gen.ts.

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

export type LinkState =
  | { state: 'connected'; port: string; since_ms: number; protocol: number }
  | { state: 'connecting'; port: string }
  | { state: 'offline' }
  | { state: 'lost'; reason: string; detail: string; at_ms: number }
  | { state: 'disconnected' }
  | { state: 'busy'; port: string; detail: string }
  | { state: 'unresponsive'; port: string }
  | { state: 'updating'; job: string };

export type FirmwareInfo = { version: string; protocol: number; build: string; recovery: boolean; update_available: string | null };
export type KeyMirrorState = { enabled: boolean; reason: string | null };

export type DeviceSummary = {
  id: string;
  rev: number;
  name: string | null;
  setup: string;
  link: LinkState;
  firmware: FirmwareInfo | null;
  capabilities: JsonValue | null;
  orientation: string | null;
  layout: JsonValue | null;
  key_mirror: KeyMirrorState;
  stable_id: boolean;
};

export type SystemInfo = { version: string; build: string; api: string; mock: boolean; data_dir_override: boolean };
export type ErrorBody = { code: string; message: string; details: JsonValue | null };
export type ClientInfo = { id: string; kind: string };

export const endpoints = {
  'devices.list': { method: 'GET', path: '/devices' },
  'devices.get': { method: 'GET', path: '/devices/{device}' },
  'system.get': { method: 'GET', path: '/system' },
  'system.ping': { method: 'GET', path: '/system/ping' },
} as const;

export type ErrorCode =
  | 'security.host'
  | 'security.origin'
  | 'auth.missing'
  | 'auth.invalid'
  | 'auth.revoked'
  | 'auth.expired'
  | 'auth.forbidden'
  | 'request.invalid'
  | 'request.too_large'
  | 'request.media_type'
  | 'limits.exceeded'
  | 'route.not_found'
  | 'route.method_not_allowed'
  | 'device.not_found'
  | 'internal';

export type SyncData = { devices: DeviceSummary[] };

export type EventMap = {
  hello: Record<string, unknown>;
  sync: SyncData;
  'device.added': DeviceSummary;
  'device.updated': DeviceSummary;
  'device.removed': { id: string };
  'auth.revoked': Record<string, unknown>;
  'companion.stopping': Record<string, unknown>;
};

export function createCompanion(base: string, token: () => string) {
  async function call<T>(name: keyof typeof endpoints, path: Record<string, string>): Promise<T> {
    const endpoint = endpoints[name];
    let route: string = endpoint.path;
    for (const [key, value] of Object.entries(path)) route = route.replace(`{${key}}`, encodeURIComponent(value));
    const headers: Record<string, string> = name === 'system.ping' ? {} : { Authorization: `Bearer ${token()}` };
    const response = await fetch(new URL(`/api/v1${route}`, base), { method: endpoint.method, headers });
    if (!response.ok) throw (await response.json()).error;
    return response.status === 204 ? (undefined as T) : response.json();
  }
  return {
    devices: {
      list: () => call<DeviceSummary[]>('devices.list', {}),
      get: (path: { device: string }) => call<DeviceSummary>('devices.get', path),
    },
    system: {
      get: () => call<SystemInfo>('system.get', {}),
      ping: () => call<{ api: string[] }>('system.ping', {}),
    },
  };
}
