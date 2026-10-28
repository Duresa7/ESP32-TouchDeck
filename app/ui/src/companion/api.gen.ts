// Generated from td-companion. Do not edit.
export type DeviceSummary = { id: string, rev: number, name: string | null, setup: string, link: LinkState, firmware: FirmwareInfo | null, capabilities: JsonValue | null, orientation: string | null, layout: JsonValue | null, key_mirror: KeyMirrorState, stable_id: boolean, };
export type FirmwareInfo = { version: string, protocol: number, build: string, recovery: boolean, update_available: string | null, };
export type LinkState = { "state": "connected", port: string, since_ms: number, protocol: number, } | { "state": "offline" } | { "state": "lost", reason: string, detail: string, at_ms: number, };
export type KeyMirrorState = { enabled: boolean, reason: string | null, };
export type DevicePath = { device: string, };
export type SystemInfo = { version: string, build: string, api: string, mock: boolean, data_dir_override: boolean, };
export type ErrorBody = { code: string, message: string, details: JsonValue | null, };
export type SessionInfo = { id: string, kind: string, label: string, created_ms: number, last_used_ms: number, current: boolean, };
export type ClientInfo = { id: string, kind: string, };
export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type ProofRequest = { nonce: string, };
export type Proof = { proof: string, };
export type SessionRequest = { kind: string, label: string | null, };
export type SessionCreated = { id: string, token: string, };
export const endpoints = {
  'devices.list': { method: 'GET', path: '/devices' },
  'devices.get': { method: 'GET', path: '/devices/{device}' },
  'system.get': { method: 'GET', path: '/system' },
  'system.shutdown': { method: 'POST', path: '/system/shutdown' },
  'system.ping': { method: 'GET', path: '/system/ping' },
  'auth.proof': { method: 'POST', path: '/auth/proof' },
  'auth.session_create': { method: 'POST', path: '/auth/sessions' },
  'auth.sessions': { method: 'GET', path: '/auth/sessions' },
  'auth.session_delete': { method: 'DELETE', path: '/auth/sessions/{id}' },
  'auth.reset': { method: 'POST', path: '/auth/reset' },
} as const;
export type ErrorCode = 'security.host' | 'security.origin' | 'auth.missing' | 'auth.invalid' | 'auth.revoked' | 'auth.forbidden' | 'request.invalid' | 'request.too_large' | 'request.media_type' | 'limits.exceeded' | 'route.not_found' | 'route.method_not_allowed' | 'device.not_found' | 'companion.stopping';
export type SyncData = { devices: DeviceSummary[] };
export type JobMap = Record<string, never>;
export type EventMap = {
  'hello': Record<string, unknown>;
  'sync': SyncData;
  'device.added': DeviceSummary;
  'device.updated': DeviceSummary;
  'device.removed': { id: string };
  'auth.revoked': Record<string, unknown>;
  'companion.stopping': Record<string, unknown>;
};

export function createCompanion(base: string, token: () => string) {
  async function call<T>(name: keyof typeof endpoints, path: Record<string, string>, query: unknown, body: unknown): Promise<T> {
    const endpoint = endpoints[name];
    let route: string = endpoint.path;
    for (const [key, value] of Object.entries(path)) route = route.replace(`{${key}}`, encodeURIComponent(value));
    const url = new URL(`/api/v1${route}`, base);
    if (query) for (const [key, value] of Object.entries(query)) url.searchParams.set(key, String(value));
    const response = await fetch(url, { method: endpoint.method, headers: { Authorization: `Bearer ${token()}`, ...(body == null ? {} : { 'Content-Type': 'application/json' }) }, ...(body == null ? {} : { body: JSON.stringify(body) }) });
    if (!response.ok) throw (await response.json()).error;
    return response.status === 204 ? undefined as T : response.json();
  }
  return {
    devices: {
      list: (path: Record<string, never> = {}, query: null = null as null, body: null = null as null) => call<Array<DeviceSummary>>('devices.list', path, query, body),
      get: (path: { device: string, }, query: null = null as null, body: null = null as null) => call<DeviceSummary>('devices.get', path, query, body),
    },
    system: {
      get: (path: Record<string, never> = {}, query: null = null as null, body: null = null as null) => call<SystemInfo>('system.get', path, query, body),
      shutdown: (path: Record<string, never> = {}, query: null = null as null, body: null = null as null) => call<null>('system.shutdown', path, query, body),
    },
    auth: {
      proof: (body: ProofRequest) => call<Proof>('auth.proof', {}, null, body),
      session_create: (body: SessionRequest) => call<SessionCreated>('auth.session_create', {}, null, body),
      sessions: () => call<SessionInfo[]>('auth.sessions', {}, null, null),
      session_delete: (id: string) => call<void>('auth.session_delete', { id }, null, null),
      reset: () => call<void>('auth.reset', {}, null, null),
    },
  };
}
