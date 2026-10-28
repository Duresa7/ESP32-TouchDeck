// Generated from td-companion. Do not edit.
export type AssetBytes = Blob;
export type AssetPath = { asset: string, };
export type AssetRef = { id: string, 
/**
 * `tdi1`, the Device's icon format.
 */
kind: string, width: number, height: number, bytes: number, };
export type AssetSource = { "kind": "app", path: string, };
export type AssetSync = { 
/**
 * Assets the Layout uses that the Device lacked and the Companion sent.
 */
sent: Array<string>, 
/**
 * Assets the Layout uses that the Device lacked and the Companion has no file for.
 */
unavailable: Array<string>, };
export type AuthRevoked = Record<string, never>;
export type Builtin = { name: string, label: string, tags: Array<string>, id: string, };
export type ClientInfo = { id: string, kind: SessionKind, };
export type CompanionStopping = { reason: string, };
export type CreateAsset = { source: AssetSource, fit?: Fit, };
export type DeviceAdded = DeviceSummary;
export type DeviceLayoutState = { source: string, name: string, orientation: Orientation, pages: number, tiles: number, icons_loaded: number | null, icons_missing: number | null, bytes: number | null, load_ms: number | null, error: string | null, };
export type DevicePath = { device: string, };
export type DeviceRemoved = { id: string, };
export type DeviceSummary = { id: string, rev: number, name: string | null, setup: string, link: LinkState, firmware: FirmwareInfo | null, capabilities: JsonValue | null, orientation: string | null, layout: JsonValue | null, key_mirror: KeyMirrorState, stable_id: boolean, };
export type DeviceUpdated = DeviceSummary;
export type ErrorBody = { code: string, message: string, details: JsonValue | null, };
export type FirmwareInfo = { version: string, protocol: number, build: string, recovery: boolean, update_available: string | null, };
export type Fit = "contain" | "cover";
export type Hello = { api: string, build: string, session: ClientInfo, };
export type Job = { id: string, kind: string, scope: JobScope, state: JobState, 
/**
 * Typed per kind in `JobMap`.
 */
progress: JsonValue, 
/**
 * Typed per kind in `JobMap`. Set once the Job succeeds.
 */
result: JsonValue | null, error: ErrorBody | null, 
/**
 * Whether the current phase allows `DELETE /jobs/{id}`.
 */
cancellable: boolean, created_ms: number, finished_ms: number | null, rev: number, };
export type JobPath = { job: string, };
export type JobScope = { "kind": "device", "id": string } | { "kind": "new-board", "id": string } | { "kind": "companion" };
export type JobState = "queued" | "waiting_for_confirmation" | "running" | "succeeded" | "failed" | "cancelled";
export type JobUpdated = Job;
export type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]?: JsonValue } | null;
export type KeyMirrorState = { enabled: boolean, reason: string | null, };
export type Layout = { schema: number, name: string, orientation: Orientation, 
/**
 * Tiles use the whole screen, with no page title or page dots. Written only when true, so a layout that
 * leaves it off stays readable by firmware that predates it.
 */
fullScreen?: boolean, pages: Array<Page>, };
export type LayoutBody = Layout;
export type LayoutPath = { device: string, orientation: Orientation, };
export type LayoutRead = { layout: Layout, 
/**
 * The Device's CRC-32 of the stored Layout as 8 hex digits, also sent as `ETag`.
 */
revision: string, };
export type LinkState = { "state": "connected", port: string, since_ms: number, protocol: number, } | { "state": "offline" } | { "state": "lost", reason: string, detail: string, at_ms: number, };
export type Modifier = "ctrl" | "shift" | "alt" | "win";
export type NoQuery = null;
export type Orientation = "landscape" | "portrait";
export type Page = { "kind": "grid", id: string, title: string, rows: number, cols: number, tiles: Array<Tile>, } | { "kind": "dashboard", id: string, title: string, source: string, };
export type Proof = { proof: string, };
export type ProofRequest = { nonce: string, };
export type PruneProgress = { removed: number, total: number, };
export type PruneRequest = { 
/**
 * Assets to keep besides the ones the Device's stored Layouts use, such as those of a
 * Layout being edited.
 */
also_keep: Array<string>, };
export type PruneResult = { removed: Array<string>, };
export type PushProgress = { assets_sent: number, assets_total: number, bytes_sent: number, bytes_total: number, };
export type PushReport = { state: DeviceLayoutState, bytes: number, chunks: number, retransmits: number, elapsed_ms: number, assets: AssetSync, 
/**
 * The pushed Layout's revision, for the next `If-Match`.
 */
revision: string, };
export type RenderProgress = { frames_done: number, frames_total: number, };
export type SessionCreated = { id: string, token: string, };
export type SessionInfo = { id: string, kind: SessionKind, label: string, created_ms: number, last_used_ms: number, current: boolean, };
export type SessionKind = "window" | "browser";
export type SessionPath = { id: string, };
export type SessionRequest = { kind: SessionKind, label: string | null, };
export type SuggestRequest = { action: TileAction, };
export type SystemInfo = { version: string, build: string, api: string, mock: boolean, data_dir_override: boolean, };
export type Tile = { id: string, label: string, row: number, col: number, rowSpan: number, colSpan: number, icon: string | null, action: TileAction, };
export type TileAction = { "type": "key", usage: number, mods: Array<Modifier>, } | { "type": "mod", mod: Modifier, } | { "type": "consumer", usage: number, } | { "type": "page", page: string, } | { "type": "none" } | { "type": "launch", path: string, args: Array<string>, cwd: string, } | { "type": "open", target: string, } | { "type": "command", program: string, args: Array<string>, cwd: string, shell: boolean, } | { "type": "widget", source: string, field: string, account: string, };
export type Versions = { api: Array<string>, };
export const endpoints = {
  'assets.builtin': { method: 'GET', path: '/assets/builtin', access: 'session' },
  'assets.create': { method: 'POST', path: '/assets', access: 'session' },
  'assets.file': { method: 'GET', path: '/assets/{asset}', access: 'session' },
  'assets.prune': { method: 'POST', path: '/devices/{device}/assets/prune', access: 'session' },
  'assets.suggest': { method: 'POST', path: '/assets/builtin/suggest', access: 'session' },
  'auth.proof': { method: 'POST', path: '/auth/proof', access: 'public' },
  'auth.reset': { method: 'POST', path: '/auth/reset', access: 'session' },
  'auth.session_create': { method: 'POST', path: '/auth/sessions', access: 'pairing' },
  'auth.session_delete': { method: 'DELETE', path: '/auth/sessions/{id}', access: 'session' },
  'auth.sessions': { method: 'GET', path: '/auth/sessions', access: 'session' },
  'devices.get': { method: 'GET', path: '/devices/{device}', access: 'session' },
  'devices.list': { method: 'GET', path: '/devices', access: 'session' },
  'jobs.cancel': { method: 'DELETE', path: '/jobs/{job}', access: 'session' },
  'jobs.get': { method: 'GET', path: '/jobs/{job}', access: 'session' },
  'jobs.list': { method: 'GET', path: '/jobs', access: 'session' },
  'layouts.get': { method: 'GET', path: '/devices/{device}/layouts/{orientation}', access: 'session' },
  'layouts.push': { method: 'PUT', path: '/devices/{device}/layouts/{orientation}', access: 'session' },
  'system.get': { method: 'GET', path: '/system', access: 'session' },
  'system.ping': { method: 'GET', path: '/system/ping', access: 'public' },
  'system.shutdown': { method: 'POST', path: '/system/shutdown', access: 'session' },
} as const;
export type ErrorCode = 'asset.not_found' | 'asset.render_failed' | 'auth.forbidden' | 'auth.invalid' | 'auth.missing' | 'auth.revoked' | 'device.busy' | 'device.link' | 'device.not_connected' | 'device.not_found' | 'device.timeout' | 'internal' | 'job.cancelled' | 'job.not_cancellable' | 'job.not_found' | 'layout.changed' | 'layout.invalid' | 'layout.not_found' | 'layout.orientation_mismatch' | 'layout.stored_invalid' | 'limits.exceeded' | 'request.invalid' | 'request.media_type' | 'request.timeout' | 'request.too_large' | 'route.method_not_allowed' | 'route.not_found' | 'security.host' | 'security.origin';
export type SyncData = {
  devices: Array<DeviceSummary>,
  jobs: Array<Job>,
};
export type JobMap = {
  'assets.prune': { progress: PruneProgress; result: PruneResult };
  'assets.render': { progress: RenderProgress; result: AssetRef };
  'layouts.push': { progress: PushProgress; result: PushReport };
};
export type EventMap = {
  'hello': Hello;
  'sync': SyncData;
  'auth.revoked': AuthRevoked;
  'companion.stopping': CompanionStopping;
  'device.added': DeviceAdded;
  'device.updated': DeviceUpdated;
  'device.removed': DeviceRemoved;
  'job.updated': JobUpdated;
};

export function createCompanion(base: string, token: () => string) {
  async function call<T>(name: keyof typeof endpoints, path: Record<string, string>, query: unknown, body: unknown, bearer?: string, headers?: Record<string, string>): Promise<T> {
    const endpoint = endpoints[name];
    let route: string = endpoint.path;
    for (const [key, value] of Object.entries(path)) route = route.replace(`{${key}}`, encodeURIComponent(value));
    const url = new URL(`/api/v1${route}`, base);
    if (query) for (const [key, value] of Object.entries(query)) url.searchParams.set(key, String(value));
    const auth = bearer ?? (endpoint.access === 'session' ? token() : undefined);
    const response = await fetch(url, { method: endpoint.method, headers: { ...headers, ...(auth ? { Authorization: `Bearer ${auth}` } : {}), ...(body == null ? {} : { 'Content-Type': 'application/json' }) }, ...(body == null ? {} : { body: JSON.stringify(body) }) });
    if (!response.ok) throw (await response.json()).error;
    if (response.status === 204) return undefined as T;
    // Asset files come back as raw bytes with their media type.
    return (response.headers.get('Content-Type') ?? '').startsWith('application/json') ? response.json() : response.blob() as T;
  }
  /** Resolves with the Job's typed result once it succeeds, and rejects with its error otherwise. */
  async function wait<K extends keyof JobMap>(job: Job, every = 250): Promise<JobMap[K]['result']> {
    for (;;) {
      if (job.state === 'succeeded') return job.result as JobMap[K]['result'];
      if (job.state === 'failed' || job.state === 'cancelled') throw job.error;
      await new Promise((resolve) => setTimeout(resolve, every));
      job = await call<Job>('jobs.get', { job: job.id }, null, null);
    }
  }
  return {
    assets: {
      builtin: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<Builtin>>('assets.builtin', path, query, body, bearer, headers),
      create: (path: Record<string, never> = {}, query: null = null, body: { source: AssetSource, fit?: Fit, }, bearer?: string, headers?: Record<string, string>) => call<Job>('assets.create', path, query, body, bearer, headers),
      file: (path: { asset: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<AssetBytes>('assets.file', path, query, body, bearer, headers),
      prune: (path: { device: string, }, query: null = null, body: { 
/**
 * Assets to keep besides the ones the Device's stored Layouts use, such as those of a
 * Layout being edited.
 */
also_keep: Array<string>, }, bearer?: string, headers?: Record<string, string>) => call<Job>('assets.prune', path, query, body, bearer, headers),
      suggest: (path: Record<string, never> = {}, query: null = null, body: { action: TileAction, }, bearer?: string, headers?: Record<string, string>) => call<Builtin | null>('assets.suggest', path, query, body, bearer, headers),
    },
    auth: {
      proof: (path: Record<string, never> = {}, query: null = null, body: { nonce: string, }, bearer?: string, headers?: Record<string, string>) => call<Proof>('auth.proof', path, query, body, bearer, headers),
      reset: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('auth.reset', path, query, body, bearer, headers),
      session_create: (path: Record<string, never> = {}, query: null = null, body: { kind: SessionKind, label: string | null, }, bearer?: string, headers?: Record<string, string>) => call<SessionCreated>('auth.session_create', path, query, body, bearer, headers),
      session_delete: (path: { id: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('auth.session_delete', path, query, body, bearer, headers),
      sessions: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<SessionInfo>>('auth.sessions', path, query, body, bearer, headers),
    },
    devices: {
      get: (path: { device: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<DeviceSummary>('devices.get', path, query, body, bearer, headers),
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<DeviceSummary>>('devices.list', path, query, body, bearer, headers),
    },
    jobs: {
      cancel: (path: { job: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Job>('jobs.cancel', path, query, body, bearer, headers),
      get: (path: { job: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Job>('jobs.get', path, query, body, bearer, headers),
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<Job>>('jobs.list', path, query, body, bearer, headers),
      wait,
    },
    layouts: {
      get: (path: { device: string, orientation: Orientation, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<LayoutRead>('layouts.get', path, query, body, bearer, headers),
      push: (path: { device: string, orientation: Orientation, }, query: null = null, body: Layout, bearer?: string, headers?: Record<string, string>) => call<Job>('layouts.push', path, query, body, bearer, headers),
    },
    system: {
      get: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<SystemInfo>('system.get', path, query, body, bearer, headers),
      ping: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Versions>('system.ping', path, query, body, bearer, headers),
      shutdown: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('system.shutdown', path, query, body, bearer, headers),
    },
  };
}
