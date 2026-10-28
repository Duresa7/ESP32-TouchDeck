// Generated from td-companion. Do not edit.
export type AuthRevoked = Record<string, never>;
export type ClientInfo = { id: string, kind: SessionKind, };
export type CompanionStopping = { reason: string, };
export type DeviceAdded = DeviceSummary;
export type DevicePath = { device: string, };
export type DeviceRemoved = { id: string, };
export type DeviceSummary = { id: string, rev: number, name: string | null, setup: string, link: LinkState, firmware: FirmwareInfo | null, capabilities: JsonValue | null, orientation: string | null, layout: JsonValue | null, key_mirror: KeyMirrorState, stable_id: boolean, };
export type DeviceUpdated = DeviceSummary;
export type FirmwareInfo = { version: string, protocol: number, build: string, recovery: boolean, update_available: string | null, };
export type Hello = { api: string, build: string, session: ClientInfo, };
export type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]?: JsonValue } | null;
export type KeyMirrorState = { enabled: boolean, reason: string | null, };
export type LinkState = { "state": "connected", port: string, since_ms: number, protocol: number, } | { "state": "offline" } | { "state": "lost", reason: string, detail: string, at_ms: number, };
export type NoQuery = null;
export type Proof = { proof: string, };
export type ProofRequest = { nonce: string, };
export type SessionCreated = { id: string, token: string, };
export type SessionInfo = { id: string, kind: SessionKind, label: string, created_ms: number, last_used_ms: number, current: boolean, };
export type SessionKind = "window" | "browser";
export type SessionPath = { id: string, };
export type SessionRequest = { kind: SessionKind, label: string | null, };
export type SystemInfo = { version: string, build: string, api: string, mock: boolean, data_dir_override: boolean, };
export type Versions = { api: Array<string>, };
export const endpoints = {
  'auth.proof': { method: 'POST', path: '/auth/proof', access: 'public' },
  'auth.reset': { method: 'POST', path: '/auth/reset', access: 'session' },
  'auth.session_create': { method: 'POST', path: '/auth/sessions', access: 'pairing' },
  'auth.session_delete': { method: 'DELETE', path: '/auth/sessions/{id}', access: 'session' },
  'auth.sessions': { method: 'GET', path: '/auth/sessions', access: 'session' },
  'devices.get': { method: 'GET', path: '/devices/{device}', access: 'session' },
  'devices.list': { method: 'GET', path: '/devices', access: 'session' },
  'system.get': { method: 'GET', path: '/system', access: 'session' },
  'system.ping': { method: 'GET', path: '/system/ping', access: 'public' },
  'system.shutdown': { method: 'POST', path: '/system/shutdown', access: 'session' },
} as const;
export type ErrorCode = 'auth.forbidden' | 'auth.invalid' | 'auth.missing' | 'auth.revoked' | 'device.not_found' | 'internal' | 'limits.exceeded' | 'request.invalid' | 'request.media_type' | 'request.timeout' | 'request.too_large' | 'route.method_not_allowed' | 'route.not_found' | 'security.host' | 'security.origin';
export type SyncData = {
  devices: Array<DeviceSummary>,
};
export type JobMap = Record<string, never>;
export type EventMap = {
  'hello': Hello;
  'sync': SyncData;
  'auth.revoked': AuthRevoked;
  'companion.stopping': CompanionStopping;
  'device.added': DeviceAdded;
  'device.updated': DeviceUpdated;
  'device.removed': DeviceRemoved;
};

export function createCompanion(base: string, token: () => string) {
  async function call<T>(name: keyof typeof endpoints, path: Record<string, string>, query: unknown, body: unknown, bearer?: string): Promise<T> {
    const endpoint = endpoints[name];
    let route: string = endpoint.path;
    for (const [key, value] of Object.entries(path)) route = route.replace(`{${key}}`, encodeURIComponent(value));
    const url = new URL(`/api/v1${route}`, base);
    if (query) for (const [key, value] of Object.entries(query)) url.searchParams.set(key, String(value));
    const auth = bearer ?? (endpoint.access === 'session' ? token() : undefined);
    const response = await fetch(url, { method: endpoint.method, headers: { ...(auth ? { Authorization: `Bearer ${auth}` } : {}), ...(body == null ? {} : { 'Content-Type': 'application/json' }) }, ...(body == null ? {} : { body: JSON.stringify(body) }) });
    if (!response.ok) throw (await response.json()).error;
    return response.status === 204 ? undefined as T : response.json();
  }
  return {
    auth: {
      proof: (path: Record<string, never> = {}, query: null = null, body: { nonce: string, }, bearer?: string) => call<Proof>('auth.proof', path, query, body, bearer),
      reset: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<null>('auth.reset', path, query, body, bearer),
      session_create: (path: Record<string, never> = {}, query: null = null, body: { kind: SessionKind, label: string | null, }, bearer?: string) => call<SessionCreated>('auth.session_create', path, query, body, bearer),
      session_delete: (path: { id: string, }, query: null = null, body: null = null, bearer?: string) => call<null>('auth.session_delete', path, query, body, bearer),
      sessions: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<Array<SessionInfo>>('auth.sessions', path, query, body, bearer),
    },
    devices: {
      get: (path: { device: string, }, query: null = null, body: null = null, bearer?: string) => call<DeviceSummary>('devices.get', path, query, body, bearer),
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<Array<DeviceSummary>>('devices.list', path, query, body, bearer),
    },
    system: {
      get: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<SystemInfo>('system.get', path, query, body, bearer),
      ping: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<Versions>('system.ping', path, query, body, bearer),
      shutdown: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<null>('system.shutdown', path, query, body, bearer),
    },
  };
}
