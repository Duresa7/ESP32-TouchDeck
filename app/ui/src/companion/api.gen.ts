// Generated from td-companion. Do not edit.
export type AuthRevoked = Record<string, never>;
export type Capabilities = { screen: [number, number], grid: [number, number], pages: number, tiles: number, layout_bytes: number, storage_free: number, tile_kinds: Array<string>, asset_bytes: number, formats: Array<string>, image: [number, number], };
export type ClientInfo = { id: string, kind: SessionKind, };
export type CompanionStopping = { reason: string, };
export type ConsoleLine = { line: string, };
export type DeviceAdded = DeviceSummary;
export type DeviceConsole = { device: DeviceId, line: string, };
export type DeviceEvent = { device: DeviceId, event: JsonValue, };
export type DeviceId = string;
export type DeviceKeyMirror = { device: DeviceId, seq: number, report: string, 
/**
 * `typed`, `duplicate`, `off`, `unknown` or `suppressed`.
 */
outcome: string, 
/**
 * The sequence numbers that never arrived, whose held keys were released.
 */
gap: [number, number] | null, errors: Array<string>, arrived_ms: number, };
export type DevicePath = { device: DeviceId, };
export type DeviceRemoved = { id: DeviceId, };
export type DeviceSettings = { idle_timeout_s: number, 
/**
 * `landscape` or `portrait`.
 */
orientation: string, name: string | null, brightness: number | null, };
export type DeviceSettingsPatch = { idle_timeout_s: number | null, orientation: string | null, name: string | null, brightness: number | null, };
export type DeviceSummary = { id: DeviceId, rev: number, name: string | null, setup: SetupState, link: LinkState, firmware: FirmwareInfo | null, capabilities: Capabilities | null, orientation: string | null, layout: JsonValue | null, key_mirror: KeyMirrorState, stable_id: boolean, };
export type DeviceUpdated = DeviceSummary;
export type FirmwareInfo = { version: string, protocol: number, build: string, recovery: boolean, update_available: string | null, };
export type Hello = { api: string, build: string, session: ClientInfo, };
export type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]?: JsonValue } | null;
export type KeyMirrorState = { enabled: boolean, reason: string | null, };
export type Lagged = { topic: Topic, device: string, dropped: number, };
export type LinkState = { "state": "connected", port: string, since_ms: number, protocol: number, } | { "state": "connecting", port: string, } | { "state": "offline" } | { "state": "lost", reason: LostReason, detail: string, at_ms: number, } | { "state": "disconnected" } | { "state": "busy", port: string, detail: string, } | { "state": "unresponsive", port: string, } | { "state": "updating", job: string, };
export type LostReason = "unplugged" | "timeout" | "io-error" | "closed";
export type NewBoard = { id: DeviceId, port: string, seen_ms: number, } & ({ "probe": "silent" } | { "probe": "busy", detail: string, } | { "probe": "other-firmware", product: string, detail: string, });
export type NewBoardAdded = NewBoard;
export type NewBoardRemoved = { id: DeviceId, };
export type NoQuery = null;
export type PortProbe = { port: string, usb_serial: string | null, id: DeviceId, probed_ms: number | null, } & ({ "status": "pending" } | { "status": "device", protocol: number, version: string, } | { "status": "silent" } | { "status": "busy", detail: string, } | { "status": "other-firmware", product: string, detail: string, });
export type Proof = { proof: string, };
export type ProofRequest = { nonce: string, };
export type SessionCreated = { id: string, token: string, };
export type SessionInfo = { id: string, kind: SessionKind, label: string, created_ms: number, last_used_ms: number, current: boolean, };
export type SessionKind = "window" | "browser";
export type SessionPath = { id: string, };
export type SessionRequest = { kind: SessionKind, label: string | null, };
export type SettingsPatched = { settings: DeviceSettings, 
/**
 * The Device must restart before an orientation change shows.
 */
restart_required: boolean, };
export type SetupState = "needs-setup" | "ready";
export type SystemInfo = { version: string, build: string, api: string, mock: boolean, data_dir_override: boolean, };
export type Topic = "state" | "console" | "key-mirror" | "device-events";
export type Versions = { api: Array<string>, };
export const endpoints = {
  'auth.proof': { method: 'POST', path: '/auth/proof', access: 'public' },
  'auth.reset': { method: 'POST', path: '/auth/reset', access: 'session' },
  'auth.session_create': { method: 'POST', path: '/auth/sessions', access: 'pairing' },
  'auth.session_delete': { method: 'DELETE', path: '/auth/sessions/{id}', access: 'session' },
  'auth.sessions': { method: 'GET', path: '/auth/sessions', access: 'session' },
  'console.send': { method: 'POST', path: '/devices/{device}/console', access: 'session' },
  'devices.connect': { method: 'POST', path: '/devices/{device}/connect', access: 'session' },
  'devices.disconnect': { method: 'POST', path: '/devices/{device}/disconnect', access: 'session' },
  'devices.forget': { method: 'DELETE', path: '/devices/{device}', access: 'session' },
  'devices.get': { method: 'GET', path: '/devices/{device}', access: 'session' },
  'devices.list': { method: 'GET', path: '/devices', access: 'session' },
  'devices.restart': { method: 'POST', path: '/devices/{device}/restart', access: 'session' },
  'devices.settings_get': { method: 'GET', path: '/devices/{device}/settings', access: 'session' },
  'devices.settings_patch': { method: 'PATCH', path: '/devices/{device}/settings', access: 'session' },
  'new_boards.list': { method: 'GET', path: '/new-boards', access: 'session' },
  'ports.list': { method: 'GET', path: '/ports', access: 'session' },
  'ports.rescan': { method: 'POST', path: '/ports/rescan', access: 'session' },
  'system.get': { method: 'GET', path: '/system', access: 'session' },
  'system.ping': { method: 'GET', path: '/system/ping', access: 'public' },
  'system.shutdown': { method: 'POST', path: '/system/shutdown', access: 'session' },
} as const;
export type ErrorCode = 'auth.forbidden' | 'auth.invalid' | 'auth.missing' | 'auth.revoked' | 'console.forbidden' | 'device.connected' | 'device.link' | 'device.needs_firmware' | 'device.not_connected' | 'device.not_found' | 'device.timeout' | 'internal' | 'limits.exceeded' | 'request.invalid' | 'request.media_type' | 'request.timeout' | 'request.too_large' | 'route.method_not_allowed' | 'route.not_found' | 'security.host' | 'security.origin';
export type SyncData = {
  devices: Array<DeviceSummary>,
  new_boards: Array<NewBoard>,
};
export type JobMap = Record<string, never>;
export type EventMap = {
  'hello': Hello;
  'sync': SyncData;
  'auth.revoked': AuthRevoked;
  'companion.stopping': CompanionStopping;
  'lagged': Lagged;
  'device.console': DeviceConsole;
  'device.added': DeviceAdded;
  'device.updated': DeviceUpdated;
  'device.removed': DeviceRemoved;
  'device.key-mirror': DeviceKeyMirror;
  'device.event': DeviceEvent;
  'new-board.added': NewBoardAdded;
  'new-board.removed': NewBoardRemoved;
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
    console: {
      send: (path: { device: DeviceId, }, query: null = null, body: { line: string, }, bearer?: string) => call<null>('console.send', path, query, body, bearer),
    },
    devices: {
      connect: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string) => call<DeviceSummary>('devices.connect', path, query, body, bearer),
      disconnect: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string) => call<DeviceSummary>('devices.disconnect', path, query, body, bearer),
      forget: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string) => call<null>('devices.forget', path, query, body, bearer),
      get: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string) => call<DeviceSummary>('devices.get', path, query, body, bearer),
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<Array<DeviceSummary>>('devices.list', path, query, body, bearer),
      restart: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string) => call<null>('devices.restart', path, query, body, bearer),
      settings_get: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string) => call<DeviceSettings>('devices.settings_get', path, query, body, bearer),
      settings_patch: (path: { device: DeviceId, }, query: null = null, body: { idle_timeout_s: number | null, orientation: string | null, name: string | null, brightness: number | null, }, bearer?: string) => call<SettingsPatched>('devices.settings_patch', path, query, body, bearer),
    },
    new_boards: {
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<Array<NewBoard>>('new_boards.list', path, query, body, bearer),
    },
    ports: {
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<Array<PortProbe>>('ports.list', path, query, body, bearer),
      rescan: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<null>('ports.rescan', path, query, body, bearer),
    },
    system: {
      get: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<SystemInfo>('system.get', path, query, body, bearer),
      ping: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<Versions>('system.ping', path, query, body, bearer),
      shutdown: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string) => call<null>('system.shutdown', path, query, body, bearer),
    },
  };
}
