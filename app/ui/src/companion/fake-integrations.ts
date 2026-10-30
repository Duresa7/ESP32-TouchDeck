import type { Delivery, IntegrationSummary, IntegrationView, JsonValue } from './api.ts';

type Frame = Record<string, unknown> & { type: string };

export interface FakeIntegrationsOptions {
  publish(frame: Frame): void;
  /** CLIProxyAPI starts set up, on and connected to a hub. Otherwise it starts as a fresh install. */
  configured?: boolean;
  /** Delivery per Device at the start. */
  delivery?: Delivery[];
  now?: () => number;
}

const json = (status: number, body: unknown) =>
  new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

const failure = (status: number, code: string, message: string, details: unknown = null) => json(status, { error: { code, message, details } });

const invalid = () => failure(422, 'integration.config_invalid', 'Invalid Integration config.', { issues: [{ message: 'Check the Integration fields.' }] });

const ID = 'cliproxyapi';
const SECRET = 'management_key';
const DEFAULT_URL = 'http://127.0.0.1:8317';
const REFRESH_FLOOR_MS = 15_000;

const STREAMS: [string, string, string | null][] = [
  ['status', 'text', null],
  ['success', 'numeric', 'requests'],
  ['failed', 'numeric', 'requests'],
  ['recent_10m', 'numeric', 'requests'],
  ['five_hour_pct', 'numeric', '%'],
  ['seven_day_pct', 'numeric', '%'],
  ['five_hour_reset', 'text', null],
  ['seven_day_reset', 'text', null],
  ['plan', 'text', null],
];

function sampleAccounts() {
  const window = (pct: number, reset_in: string) => ({ pct, reset_in });
  return [
    { key: 'codex:alice@example.com', provider: 'codex', status: 'active', plan: 'pro', success: 128, failed: 2, recent_10m: 6, five_hour: window(42, '2h 10m'), seven_day: window(10, '3d 4h'), usage_error: null },
    { key: 'claude:alice@example.com', provider: 'claude', status: 'active', plan: null, success: 64, failed: 0, recent_10m: 5, five_hour: window(24, '2h 10m'), seven_day: window(61, '3d 4h'), usage_error: null },
    { key: 'gemini-cli:linus@example.com', provider: 'gemini-cli', status: 'active', plan: null, success: 40, failed: 1, recent_10m: 0, five_hour: null, seven_day: null, usage_error: null },
  ];
}

function originOf(address: string): string | null {
  let url: URL;
  try {
    url = new URL(address.trim());
  } catch {
    return null;
  }
  if ((url.protocol !== 'http:' && url.protocol !== 'https:') || !url.hostname || url.username || url.password || url.search || url.hash) return null;
  return url.origin;
}

interface Settings {
  enabled: boolean;
  url: string;
  interval_s: number;
}

function parseSettings(body: unknown): Settings | null {
  if (typeof body !== 'object' || body === null || Array.isArray(body)) return null;
  const fields = new Map<string, unknown>(Object.entries(body));
  if ([...fields.keys()].sort().join(',') !== 'enabled,interval_s,url') return null;
  const [enabled, url, interval_s] = [fields.get('enabled'), fields.get('url'), fields.get('interval_s')];
  if (typeof enabled !== 'boolean' || typeof url !== 'string' || typeof interval_s !== 'number' || !Number.isInteger(interval_s) || interval_s < 0) return null;
  return { enabled, url, interval_s };
}

export function createFakeIntegrations(options: FakeIntegrationsOptions) {
  const { publish, now = Date.now } = options;
  const configured = options.configured ?? false;
  let settings: Settings = configured ? { enabled: true, url: 'https://hub.example.com', interval_s: 300 } : { enabled: false, url: DEFAULT_URL, interval_s: 300 };
  let bound = originOf(settings.url);
  let keyOrigin: string | null = configured ? bound : null;
  let hubUp = true;
  let polled: number | null = configured ? now() - 12_000 : null;
  let lastRefresh: number | null = null;
  let refreshes = 0;
  let configPuts = 0;
  const delivery = new Map((options.delivery ?? []).map((d) => [d.device, d]));

  const keySet = () => keyOrigin !== null && keyOrigin === bound;

  function summary(): IntegrationSummary {
    const state = !keySet() ? 'not_configured' : hubUp ? 'ok' : 'error';
    return {
      id: ID,
      name: 'CLIProxyAPI',
      icon: 'activity',
      enabled: settings.enabled,
      status: { state, message: state === 'error' ? 'connection refused' : null, delivery: [...delivery.values()] },
      config_schema: [
        { name: 'enabled', kind: 'boolean', secret: false },
        { name: 'url', kind: 'string', secret: false },
        { name: 'interval_s', kind: 'integer', secret: false },
        { name: SECRET, kind: 'string', secret: true },
      ],
      streams: STREAMS.map(([id, kind, unit]) => ({ id, kind, unit })),
      actions: ['refresh'],
      presets: STREAMS.map(([id]) => ({ id, name: id.replaceAll('_', ' '), action: { type: 'widget', source: ID, field: id, account: '' } })),
    };
  }

  function usage(): JsonValue {
    const working = settings.enabled && keySet();
    return {
      polled_ms: working ? polled : null,
      fails: working && !hubUp ? 3 : 0,
      stale: working && !hubUp,
      accounts: working && hubUp && polled !== null ? sampleAccounts() : [],
    };
  }

  const view = (): IntegrationView & { usage: JsonValue } => ({ summary: summary(), config: { ...settings }, secrets: { [SECRET]: { set: keySet() } }, usage: usage() });

  const changed = () => publish({ type: 'integration.status', data: summary() });

  async function body(request: Request): Promise<unknown> {
    try {
      return JSON.parse(await request.text());
    } catch {
      return undefined;
    }
  }

  return {
    list: () => [summary()],

    /** Answers a session-checked request, or returns null when it isn't an Integrations route. */
    async handle(request: Request, path: string): Promise<Response | null> {
      const method = request.method;
      if (path === '/integrations') return method === 'GET' ? json(200, [summary()]) : failure(405, 'route.method_not_allowed', 'Use GET.');
      const match = /^\/integrations\/([^/]+)(?:\/(config|refresh|secrets\/([^/]+)))?$/.exec(path);
      if (!match) return null;
      if (decodeURIComponent(match[1]) !== ID) return failure(404, 'integration.not_found', 'The Integration does not exist.');
      const [, , part, field] = match;
      if (!part) return method === 'GET' ? json(200, view()) : failure(405, 'route.method_not_allowed', 'Use GET.');
      if (part === 'config') {
        if (method !== 'PUT') return failure(405, 'route.method_not_allowed', 'Use PUT.');
        const next = parseSettings(await body(request));
        if (!next) return invalid();
        const url = next.url.trim().replace(/\/+$/, '');
        const origin = originOf(url);
        if (!origin) return invalid();
        if (origin !== bound) keyOrigin = null;
        bound = origin;
        settings = { enabled: next.enabled, url, interval_s: Math.max(60, next.interval_s) };
        configPuts += 1;
        if (settings.enabled && keySet()) polled = now();
        changed();
        return json(200, view());
      }
      if (part === 'refresh') {
        if (method !== 'POST') return failure(405, 'route.method_not_allowed', 'Use POST.');
        if (settings.enabled && (lastRefresh === null || now() - lastRefresh >= REFRESH_FLOOR_MS)) {
          lastRefresh = now();
          refreshes += 1;
          polled = now();
          changed();
        }
        return json(204, null);
      }
      if (decodeURIComponent(field) !== SECRET) return invalid();
      if (method === 'DELETE') {
        keyOrigin = null;
        changed();
        return json(204, null);
      }
      if (method !== 'PUT') return failure(405, 'route.method_not_allowed', 'Use PUT or DELETE.');
      const input = await body(request);
      const keys = typeof input === 'object' && input !== null ? Object.keys(input) : [];
      const value = typeof input === 'object' && input !== null && 'value' in input ? input.value : undefined;
      if (keys.length !== 1 || typeof value !== 'string' || !value.trim() || value.length > 4096 || /\p{Cc}/u.test(value)) return invalid();
      keyOrigin = bound;
      if (settings.enabled) polled = now();
      changed();
      return json(200, { hint: 'Set' });
    },

    /** `GET /test/integrations/record`: what the fake was asked to do, for the demo's Playwright specs. */
    test(request: Request, path: string): Response {
      if (path === '/test/integrations/record' && request.method === 'GET') return json(200, { settings, keySet: keySet(), refreshes, configPuts });
      return failure(404, 'route.not_found', 'No test route with that path.');
    },

    /** The hub stops or starts answering. */
    setHub(up: boolean) {
      hubUp = up;
      changed();
    },

    /** The Device scheduler reports a delivery. */
    deliver(entry: Delivery) {
      delivery.set(entry.device, entry);
      changed();
    },

    get refreshes() {
      return refreshes;
    },
  };
}

export type FakeIntegrations = ReturnType<typeof createFakeIntegrations>;
