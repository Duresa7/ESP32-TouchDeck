// Generated from td-companion. Do not edit.
export type AccountUsage = { provider: string, account: string, status: string, success: number, failed: number, recent_10m: number, plan: string | null, five_hour: UsageWindow | null, seven_day: UsageWindow | null, error: string | null, };
export type ActionLog = { id: string, device: DeviceId, at_ms: number, page: string, tile: string, kind: string, status: string, detail: string, pid: number | null, exit_code: number | null, };
export type ActionQuery = { device: DeviceId | null, };
export type AllowedAction = { hash: string, granted_ms: number, kind: string, program: string, args: Array<string>, cwd: string, shell: boolean, };
export type AllowedPath = { hash: string, };
export type Answer = "once" | "always" | "deny" | "expired";
export type ApprovalPath = { id: string, };
export type ApprovalRequest = { id: string, device: DeviceId, page: string, tile: string, hash: string, created_ms: number, kind: string, program: string, args: Array<string>, cwd: string, shell: boolean, };
export type ApprovalResolved = { id: string, answer: Answer, };
export type AssetBytes = Blob;
export type AssetPath = { asset: string, };
export type AssetRef = { id: string, 
/**
 * `tdi1`, the Device's icon format.
 */
kind: string, width: number, height: number, bytes: number, };
export type AssetSource = { "kind": "app", path: string, } | { "kind": "catalog", catalog: string, id: string, variant: CatalogVariant, padding: number, background: [number, number, number, number] | null, tint: [number, number, number] | null, } | { "kind": "upload", upload: string, };
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
export type BrowserKind = "browser";
export type Builtin = { name: string, label: string, tags: Array<string>, id: string, };
export type BundleInfo = { id: string, version: string, build: string, bytes: number, };
export type Capabilities = { screen: [number, number], grid: [number, number], pages: number, tiles: number, layout_bytes: number, layout_schemas: Array<number>, storage_free: number, tile_kinds: Array<string>, asset_bytes: number, formats: Array<string>, image: [number, number], };
export type CatalogHit = { source: string, name: string, id: string, aliases: Array<string>, hex: string | null, tags: Array<string>, category: string | null, svg: boolean, light: boolean, dark: boolean, license: string, license_url: string | null, guidelines: string | null, attribution_url: string, attribution: string, };
export type CatalogQuery = { q: string, catalog: string | null, };
export type CatalogVariant = "base" | "light" | "dark";
export type CliProxyDetail = { accounts: Array<AccountUsage>, polled_ms: number, };
export type ClientInfo = { id: string, kind: SessionKind, };
export type CompanionStopping = { reason: string, };
export type ConfigField = { name: string, kind: string, secret: boolean, };
export type ConsoleLine = { line: string, };
export type CreateAsset = { source: AssetSource, fit?: Fit, };
export type DataStream = { id: string, kind: string, unit: string | null, };
export type Delivery = { device: string, sent: number, error: string | null, };
export type DeviceAdded = DeviceSummary;
export type DeviceAssets = { ids: Array<string>, storage_free: number, };
export type DeviceConsole = { device: DeviceId, line: string, };
export type DeviceDiagnostics = { product: string, running_from: string, sinks: Array<string>, current_layout: string, };
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
export type DeviceLayoutState = { source: string, name: string, orientation: Orientation, pages: number, tiles: number, icons_loaded: number | null, icons_missing: number | null, bytes: number | null, load_ms: number | null, error: string | null, };
export type DevicePath = { device: DeviceId, };
export type DeviceRemoved = { id: DeviceId, };
export type DeviceSettings = { idle_timeout_s: number, 
/**
 * `landscape` or `portrait`.
 */
orientation: string, name: string | null, brightness: number | null, };
export type DeviceSettingsPatch = { idle_timeout_s: number | null, orientation: string | null, name: string | null, brightness: number | null, };
export type DeviceSummary = { id: DeviceId, rev: number, name: string | null, setup: SetupState, link: LinkState, firmware: FirmwareInfo | null, diagnostics: DeviceDiagnostics | null, capabilities: Capabilities | null, orientation: string | null, layout: JsonValue | null, key_mirror: KeyMirrorState, stable_id: boolean, };
export type DeviceUpdated = DeviceSummary;
export type ErrorBody = { code: string, message: string, details: JsonValue | null, };
export type FirmwareBuild = { version: string, build: string, };
export type FirmwareInfo = { version: string, protocol: number, build: string, recovery: boolean, update_available: string | null, };
export type FirmwareProgress = { phase: string, pct: number | null, bytes: number, };
export type FirmwareRequest = { bundle: string, };
export type FirmwareResult = { message: string, version: string, };
export type FirmwareTarget = { device: DeviceId, };
export type Fit = "contain" | "cover";
export type Hello = { api: string, build: string, session: ClientInfo, };
export type InstallLock = { lock: string, };
export type IntegrationPath = { integration: string, };
export type IntegrationStatus = { state: string, message: string | null, delivery: Array<Delivery>, };
export type IntegrationStatusEvent = IntegrationSummary;
export type IntegrationSummary = { revision: number, id: string, name: string, icon: string, enabled: boolean, status: IntegrationStatus, config_schema: Array<ConfigField>, streams: Array<DataStream>, actions: Array<string>, presets: Array<PresetTile>, };
export type IntegrationView = { detail: CliProxyDetail | null, summary: IntegrationSummary, config: JsonValue, secrets: { [key in string]?: SecretState }, };
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
export type Lagged = { topic: Topic, device: string, dropped: number, };
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
export type LinkState = { "state": "connected", port: string, since_ms: number, protocol: number, } | { "state": "connecting", port: string, } | { "state": "offline" } | { "state": "lost", reason: LostReason, detail: string, at_ms: number, } | { "state": "disconnected" } | { "state": "busy", port: string, detail: string, } | { "state": "unresponsive", port: string, } | { "state": "updating", job: string, };
export type LostReason = "unplugged" | "timeout" | "io-error" | "closed";
export type Modifier = "ctrl" | "shift" | "alt" | "win";
export type NewBoard = { id: DeviceId, port: string, seen_ms: number, } & ({ "probe": "silent" } | { "probe": "busy", detail: string, } | { "probe": "other-firmware", product: string, detail: string, });
export type NewBoardAdded = NewBoard;
export type NewBoardRemoved = { id: DeviceId, };
export type NoQuery = null;
export type Orientation = "landscape" | "portrait";
export type Page = { "kind": "grid", id: string, title: string, rows: number, cols: number, tiles: Array<Tile>, } | { "kind": "dashboard", id: string, title: string, source: string, };
export type PairExpires = { expires_ms: number, };
export type PairRequest = { code: string, label: string | null, };
export type PickResult = { bundle: BundleInfo | null, };
export type PortProbe = { port: string, usb_serial: string | null, id: DeviceId, probed_ms: number | null, } & ({ "status": "pending" } | { "status": "device", protocol: number, version: string, } | { "status": "silent" } | { "status": "busy", detail: string, } | { "status": "other-firmware", product: string, detail: string, });
export type PresetTile = { id: string, name: string, action: JsonValue, };
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
export type SecretHint = { hint: string, };
export type SecretInput = { value: string, };
export type SecretPath = { integration: string, field: string, };
export type SecretState = { set: boolean, };
export type SessionCreated = { id: string, token: string, };
export type SessionInfo = { id: string, kind: SessionKind, label: string, created_ms: number, last_used_ms: number, current: boolean, };
export type SessionKind = "window" | "browser";
export type SessionPath = { id: string, };
export type SessionRequest = { kind: SessionKind, label: string | null, };
export type SessionsDeleteQuery = { kind: BrowserKind, };
export type SettingsPatched = { settings: DeviceSettings, 
/**
 * The Device must restart before an orientation change shows.
 */
restart_required: boolean, };
export type SetupState = "needs-setup" | "ready";
export type StartupSetting = { at_sign_in: boolean, };
export type SuggestRequest = { action: TileAction, };
export type SystemInfo = { version: string, build: string, api: string, mock: boolean, data_dir_override: boolean, };
export type Tile = { id: string, label: string, row: number, col: number, rowSpan: number, colSpan: number, icon: string | null, asset?: string | null, action: TileAction, };
export type TileAction = { "type": "key", usage: number, mods: Array<Modifier>, } | { "type": "mod", mod: Modifier, } | { "type": "consumer", usage: number, } | { "type": "page", page: string, } | { "type": "none" } | { "type": "launch", path: string, args: Array<string>, cwd: string, } | { "type": "open", target: string, } | { "type": "command", program: string, args: Array<string>, cwd: string, shell: boolean, } | { "type": "widget", source: string, field: string, account: string, };
export type Topic = "state" | "console" | "key-mirror" | "device-events";
export type UploadBody = Blob;
export type Uploaded = { id: string, media_type: string, bytes: number, };
export type UsageWindow = { pct: number, resets_at_ms: number | null, };
export type Versions = { api: Array<string>, };
export const endpoints = {
  'actions.allowed': { method: 'GET', path: '/actions/allowed', access: 'session' },
  'actions.allowed_remove': { method: 'DELETE', path: '/actions/allowed/{hash}', access: 'session' },
  'actions.log': { method: 'GET', path: '/actions/log', access: 'session' },
  'approvals.list': { method: 'GET', path: '/approvals', access: 'session' },
  'approvals.show': { method: 'POST', path: '/approvals/{id}/show', access: 'session' },
  'assets.builtin': { method: 'GET', path: '/assets/builtin', access: 'session' },
  'assets.catalog_search': { method: 'GET', path: '/assets/catalog/search', access: 'session' },
  'assets.create': { method: 'POST', path: '/assets', access: 'session' },
  'assets.file': { method: 'GET', path: '/assets/{asset}', access: 'session' },
  'assets.on_device': { method: 'GET', path: '/devices/{device}/assets', access: 'session' },
  'assets.prune': { method: 'POST', path: '/devices/{device}/assets/prune', access: 'session' },
  'assets.suggest': { method: 'POST', path: '/assets/builtin/suggest', access: 'session' },
  'assets.upload': { method: 'POST', path: '/uploads', access: 'session' },
  'auth.pair': { method: 'POST', path: '/auth/pair', access: 'public' },
  'auth.pair_code': { method: 'POST', path: '/auth/pair-codes', access: 'session' },
  'auth.proof': { method: 'POST', path: '/auth/proof', access: 'public' },
  'auth.reset': { method: 'POST', path: '/auth/reset', access: 'session' },
  'auth.session_create': { method: 'POST', path: '/auth/sessions', access: 'pairing' },
  'auth.session_delete': { method: 'DELETE', path: '/auth/sessions/{id}', access: 'session' },
  'auth.sessions': { method: 'GET', path: '/auth/sessions', access: 'session' },
  'auth.sessions_delete': { method: 'DELETE', path: '/auth/sessions', access: 'session' },
  'console.send': { method: 'POST', path: '/devices/{device}/console', access: 'session' },
  'devices.connect': { method: 'POST', path: '/devices/{device}/connect', access: 'session' },
  'devices.disconnect': { method: 'POST', path: '/devices/{device}/disconnect', access: 'session' },
  'devices.forget': { method: 'DELETE', path: '/devices/{device}', access: 'session' },
  'devices.get': { method: 'GET', path: '/devices/{device}', access: 'session' },
  'devices.list': { method: 'GET', path: '/devices', access: 'session' },
  'devices.restart': { method: 'POST', path: '/devices/{device}/restart', access: 'session' },
  'devices.settings_get': { method: 'GET', path: '/devices/{device}/settings', access: 'session' },
  'devices.settings_patch': { method: 'PATCH', path: '/devices/{device}/settings', access: 'session' },
  'firmware.bundled': { method: 'GET', path: '/firmware/bundled', access: 'session' },
  'firmware.install': { method: 'POST', path: '/new-boards/{device}/install', access: 'session' },
  'firmware.pick': { method: 'POST', path: '/firmware/bundles/pick', access: 'session' },
  'firmware.repair': { method: 'POST', path: '/devices/{device}/firmware/repair', access: 'session' },
  'firmware.update': { method: 'POST', path: '/devices/{device}/firmware/update', access: 'session' },
  'integrations.config_put': { method: 'PUT', path: '/integrations/{integration}/config', access: 'session' },
  'integrations.get': { method: 'GET', path: '/integrations/{integration}', access: 'session' },
  'integrations.list': { method: 'GET', path: '/integrations', access: 'session' },
  'integrations.refresh': { method: 'POST', path: '/integrations/{integration}/refresh', access: 'session' },
  'integrations.secret_delete': { method: 'DELETE', path: '/integrations/{integration}/secrets/{field}', access: 'session' },
  'integrations.secret_put': { method: 'PUT', path: '/integrations/{integration}/secrets/{field}', access: 'session' },
  'jobs.cancel': { method: 'DELETE', path: '/jobs/{job}', access: 'session' },
  'jobs.get': { method: 'GET', path: '/jobs/{job}', access: 'session' },
  'jobs.list': { method: 'GET', path: '/jobs', access: 'session' },
  'layouts.get': { method: 'GET', path: '/devices/{device}/layouts/{orientation}', access: 'session' },
  'layouts.push': { method: 'PUT', path: '/devices/{device}/layouts/{orientation}', access: 'session' },
  'new_boards.list': { method: 'GET', path: '/new-boards', access: 'session' },
  'ports.list': { method: 'GET', path: '/ports', access: 'session' },
  'ports.rescan': { method: 'POST', path: '/ports/rescan', access: 'session' },
  'system.get': { method: 'GET', path: '/system', access: 'session' },
  'system.lock': { method: 'POST', path: '/system/lock', access: 'session' },
  'system.lock_renew': { method: 'PUT', path: '/system/lock/{lock}', access: 'session' },
  'system.ping': { method: 'GET', path: '/system/ping', access: 'public' },
  'system.shutdown': { method: 'POST', path: '/system/shutdown', access: 'session' },
  'system.startup_get': { method: 'GET', path: '/system/startup', access: 'session' },
  'system.startup_put': { method: 'PUT', path: '/system/startup', access: 'session' },
  'system.unlock': { method: 'DELETE', path: '/system/lock/{lock}', access: 'session' },
} as const;
export type ErrorCode = 'action.not_found' | 'approval.not_found' | 'asset.not_found' | 'asset.render_failed' | 'auth.code_invalid' | 'auth.code_locked' | 'auth.expired' | 'auth.forbidden' | 'auth.invalid' | 'auth.missing' | 'auth.revoked' | 'companion.installing' | 'console.forbidden' | 'device.busy' | 'device.connected' | 'device.link' | 'device.needs_firmware' | 'device.not_connected' | 'device.not_found' | 'device.timeout' | 'firmware.bundle_invalid' | 'firmware.bundle_not_found' | 'firmware.confirm_timeout' | 'firmware.declined' | 'firmware.failed' | 'firmware.rolled_back' | 'integration.config_invalid' | 'integration.credentials' | 'integration.not_found' | 'internal' | 'job.cancelled' | 'job.not_cancellable' | 'job.not_found' | 'layout.changed' | 'layout.invalid' | 'layout.not_found' | 'layout.orientation_mismatch' | 'layout.stored_invalid' | 'limits.exceeded' | 'new_board.not_found' | 'request.invalid' | 'request.media_type' | 'request.timeout' | 'request.too_large' | 'route.method_not_allowed' | 'route.not_found' | 'security.host' | 'security.origin' | 'system.busy' | 'system.dialog_open' | 'upload.not_found';
export type SyncData = {
  approvals: Array<ApprovalRequest>,
  devices: Array<DeviceSummary>,
  integrations: Array<IntegrationSummary>,
  jobs: Array<Job>,
  new_boards: Array<NewBoard>,
};
export type JobMap = {
  'assets.prune': { progress: PruneProgress; result: PruneResult };
  'assets.render': { progress: RenderProgress; result: AssetRef };
  'firmware.install': { progress: FirmwareProgress; result: FirmwareResult };
  'firmware.repair': { progress: FirmwareProgress; result: FirmwareResult };
  'firmware.update': { progress: FirmwareProgress; result: FirmwareResult };
  'layouts.push': { progress: PushProgress; result: PushReport };
};
export type EventMap = {
  'hello': Hello;
  'sync': SyncData;
  'auth.revoked': AuthRevoked;
  'companion.stopping': CompanionStopping;
  'lagged': Lagged;
  'action.logged': ActionLog;
  'approval.requested': ApprovalRequest;
  'approval.resolved': ApprovalResolved;
  'device.console': DeviceConsole;
  'device.added': DeviceAdded;
  'device.updated': DeviceUpdated;
  'device.removed': DeviceRemoved;
  'device.key-mirror': DeviceKeyMirror;
  'device.event': DeviceEvent;
  'integration.status': IntegrationStatusEvent;
  'job.updated': JobUpdated;
  'new-board.added': NewBoardAdded;
  'new-board.removed': NewBoardRemoved;
};

export function createCompanion(base: string, token: () => string) {
  async function call<T>(name: keyof typeof endpoints, path: Record<string, string>, query: unknown, body: unknown, bearer?: string, headers?: Record<string, string>): Promise<T> {
    const endpoint = endpoints[name];
    let route: string = endpoint.path;
    for (const [key, value] of Object.entries(path)) route = route.replace(`{${key}}`, encodeURIComponent(value));
    const url = new URL(`/api/v1${route}`, base);
    if (query) for (const [key, value] of Object.entries(query)) url.searchParams.set(key, String(value));
    const auth = bearer ?? (endpoint.access === 'session' ? token() : undefined);
    const response = await fetch(url, { method: endpoint.method, headers: { ...headers, ...(auth ? { Authorization: `Bearer ${auth}` } : {}), ...(body == null ? {} : { 'Content-Type': body instanceof Blob ? body.type : 'application/json' }) }, ...(body == null ? {} : { body: body instanceof Blob ? body : JSON.stringify(body) }) });
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
    actions: {
      allowed: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<AllowedAction>>('actions.allowed', path, query, body, bearer, headers),
      allowed_remove: (path: { hash: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('actions.allowed_remove', path, query, body, bearer, headers),
      log: (path: Record<string, never> = {}, query: { device: DeviceId | null, }, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<ActionLog>>('actions.log', path, query, body, bearer, headers),
    },
    approvals: {
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<ApprovalRequest>>('approvals.list', path, query, body, bearer, headers),
      show: (path: { id: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('approvals.show', path, query, body, bearer, headers),
    },
    assets: {
      builtin: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<Builtin>>('assets.builtin', path, query, body, bearer, headers),
      catalog_search: (path: Record<string, never> = {}, query: { q: string, catalog: string | null, }, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<CatalogHit>>('assets.catalog_search', path, query, body, bearer, headers),
      create: (path: Record<string, never> = {}, query: null = null, body: { source: AssetSource, fit?: Fit, }, bearer?: string, headers?: Record<string, string>) => call<Job>('assets.create', path, query, body, bearer, headers),
      file: (path: { asset: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<AssetBytes>('assets.file', path, query, body, bearer, headers),
      on_device: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<DeviceAssets>('assets.on_device', path, query, body, bearer, headers),
      prune: (path: { device: DeviceId, }, query: null = null, body: { 
/**
 * Assets to keep besides the ones the Device's stored Layouts use, such as those of a
 * Layout being edited.
 */
also_keep: Array<string>, }, bearer?: string, headers?: Record<string, string>) => call<Job>('assets.prune', path, query, body, bearer, headers),
      suggest: (path: Record<string, never> = {}, query: null = null, body: { action: TileAction, }, bearer?: string, headers?: Record<string, string>) => call<Builtin | null>('assets.suggest', path, query, body, bearer, headers),
      upload: (path: Record<string, never> = {}, query: null = null, body: Blob, bearer?: string, headers?: Record<string, string>) => call<Uploaded>('assets.upload', path, query, body, bearer, headers),
    },
    auth: {
      pair: (path: Record<string, never> = {}, query: null = null, body: { code: string, label: string | null, }, bearer?: string, headers?: Record<string, string>) => call<SessionCreated>('auth.pair', path, query, body, bearer, headers),
      pair_code: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<PairExpires>('auth.pair_code', path, query, body, bearer, headers),
      proof: (path: Record<string, never> = {}, query: null = null, body: { nonce: string, }, bearer?: string, headers?: Record<string, string>) => call<Proof>('auth.proof', path, query, body, bearer, headers),
      reset: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('auth.reset', path, query, body, bearer, headers),
      session_create: (path: Record<string, never> = {}, query: null = null, body: { kind: SessionKind, label: string | null, }, bearer?: string, headers?: Record<string, string>) => call<SessionCreated>('auth.session_create', path, query, body, bearer, headers),
      session_delete: (path: { id: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('auth.session_delete', path, query, body, bearer, headers),
      sessions: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<SessionInfo>>('auth.sessions', path, query, body, bearer, headers),
      sessions_delete: (path: Record<string, never> = {}, query: { kind: BrowserKind, }, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('auth.sessions_delete', path, query, body, bearer, headers),
    },
    console: {
      send: (path: { device: DeviceId, }, query: null = null, body: { line: string, }, bearer?: string, headers?: Record<string, string>) => call<null>('console.send', path, query, body, bearer, headers),
    },
    devices: {
      connect: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<DeviceSummary>('devices.connect', path, query, body, bearer, headers),
      disconnect: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<DeviceSummary>('devices.disconnect', path, query, body, bearer, headers),
      forget: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('devices.forget', path, query, body, bearer, headers),
      get: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<DeviceSummary>('devices.get', path, query, body, bearer, headers),
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<DeviceSummary>>('devices.list', path, query, body, bearer, headers),
      restart: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('devices.restart', path, query, body, bearer, headers),
      settings_get: (path: { device: DeviceId, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<DeviceSettings>('devices.settings_get', path, query, body, bearer, headers),
      settings_patch: (path: { device: DeviceId, }, query: null = null, body: { idle_timeout_s: number | null, orientation: string | null, name: string | null, brightness: number | null, }, bearer?: string, headers?: Record<string, string>) => call<SettingsPatched>('devices.settings_patch', path, query, body, bearer, headers),
    },
    firmware: {
      bundled: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<FirmwareBuild>('firmware.bundled', path, query, body, bearer, headers),
      install: (path: { device: DeviceId, }, query: null = null, body: { bundle: string, }, bearer?: string, headers?: Record<string, string>) => call<Job>('firmware.install', path, query, body, bearer, headers),
      pick: (path: Record<string, never> = {}, query: null = null, body: { device: DeviceId, }, bearer?: string, headers?: Record<string, string>) => call<PickResult>('firmware.pick', path, query, body, bearer, headers),
      repair: (path: { device: DeviceId, }, query: null = null, body: { bundle: string, }, bearer?: string, headers?: Record<string, string>) => call<Job>('firmware.repair', path, query, body, bearer, headers),
      update: (path: { device: DeviceId, }, query: null = null, body: { bundle: string, }, bearer?: string, headers?: Record<string, string>) => call<Job>('firmware.update', path, query, body, bearer, headers),
    },
    integrations: {
      config_put: (path: { integration: string, }, query: null = null, body: number | string | boolean | Array<JsonValue> | { [key in string]?: JsonValue } | null, bearer?: string, headers?: Record<string, string>) => call<IntegrationView>('integrations.config_put', path, query, body, bearer, headers),
      get: (path: { integration: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<IntegrationView>('integrations.get', path, query, body, bearer, headers),
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<IntegrationSummary>>('integrations.list', path, query, body, bearer, headers),
      refresh: (path: { integration: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('integrations.refresh', path, query, body, bearer, headers),
      secret_delete: (path: { integration: string, field: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('integrations.secret_delete', path, query, body, bearer, headers),
      secret_put: (path: { integration: string, field: string, }, query: null = null, body: { value: string, }, bearer?: string, headers?: Record<string, string>) => call<SecretHint>('integrations.secret_put', path, query, body, bearer, headers),
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
    new_boards: {
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<NewBoard>>('new_boards.list', path, query, body, bearer, headers),
    },
    ports: {
      list: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Array<PortProbe>>('ports.list', path, query, body, bearer, headers),
      rescan: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('ports.rescan', path, query, body, bearer, headers),
    },
    system: {
      get: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<SystemInfo>('system.get', path, query, body, bearer, headers),
      lock: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<InstallLock>('system.lock', path, query, body, bearer, headers),
      lock_renew: (path: { lock: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('system.lock_renew', path, query, body, bearer, headers),
      ping: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<Versions>('system.ping', path, query, body, bearer, headers),
      shutdown: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('system.shutdown', path, query, body, bearer, headers),
      startup_get: (path: Record<string, never> = {}, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<StartupSetting>('system.startup_get', path, query, body, bearer, headers),
      startup_put: (path: Record<string, never> = {}, query: null = null, body: { at_sign_in: boolean, }, bearer?: string, headers?: Record<string, string>) => call<StartupSetting>('system.startup_put', path, query, body, bearer, headers),
      unlock: (path: { lock: string, }, query: null = null, body: null = null, bearer?: string, headers?: Record<string, string>) => call<null>('system.unlock', path, query, body, bearer, headers),
    },
  };
}
