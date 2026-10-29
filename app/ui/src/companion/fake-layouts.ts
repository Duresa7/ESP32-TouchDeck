import { canonical, parseLayout, type Layout, type Orientation } from '../lib/editor/validate.ts';
import type { DeviceSummary, Job, JobMap, JobScope, Layout as StoredLayout, LinkState, PushReport } from './api.ts';

type Frame = Record<string, unknown> & { type: string };

export interface FakeLayoutsOptions {
  devices: Map<string, DeviceSummary>;
  publish(frame: Frame): void;
  seed?: Record<string, StoredLayout>;
  delayMs?: number;
}

const json = (status: number, body: unknown, headers: Record<string, string> = {}) =>
  new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json', ...headers } });

const failure = (status: number, code: string, message: string, details: unknown = null) => json(status, { error: { code, message, details } });

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

export function revisionOf(text: string): string {
  let crc = 0xffffffff;
  for (const byte of new TextEncoder().encode(text)) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return ((crc ^ 0xffffffff) >>> 0).toString(16).padStart(8, '0');
}

const ORIENTATIONS: readonly string[] = ['landscape', 'portrait'];

export function createFakeLayouts(options: FakeLayoutsOptions) {
  const { devices, publish, delayMs = 150 } = options;
  const stored = new Map<string, string>();
  const jobs = new Map<string, Job>();
  let minted = 0;
  let puts = 0;
  const held = new Set<string>();
  const releases = new Map<string, () => void>();

  const slot = (device: string, orientation: string) => `${device}/${orientation}`;
  for (const [device, layout] of Object.entries(options.seed ?? {})) stored.set(slot(device, layout.orientation), JSON.stringify(layout));

  function update(job: Job, change: Partial<Job>) {
    const next = { ...job, ...change, rev: job.rev + 1 };
    jobs.set(next.id, next);
    publish({ type: 'job.updated', data: next });
    return next;
  }

  const active = (device: string) =>
    [...jobs.values()].find((job) => job.scope.kind === 'device' && job.scope.id === device && !['succeeded', 'failed', 'cancelled'].includes(job.state));

  function startJob<K extends keyof JobMap>(kind: K, device: string, progress: JobMap[K]['progress'], work: () => JobMap[K]['result']): Job {
    minted += 1;
    const scope: JobScope = { kind: 'device', id: device };
    const job: Job = {
      id: `job_fake${String(minted).padStart(12, '0')}`,
      kind,
      scope,
      state: 'running',
      progress,
      result: null,
      error: null,
      cancellable: true,
      created_ms: Date.now(),
      finished_ms: null,
      rev: 1,
    };
    jobs.set(job.id, job);
    publish({ type: 'job.updated', data: job });
    const finish = () => {
      const now = jobs.get(job.id)!;
      if (now.state === 'cancelled') return;
      update(now, { state: 'succeeded', result: work(), cancellable: false, finished_ms: Date.now() });
    };
    if (held.delete(device)) releases.set(device, () => setTimeout(finish, 0));
    else setTimeout(finish, delayMs);
    return job;
  }

  function deviceOr404(id: string): DeviceSummary | Response {
    return devices.get(id) ?? failure(404, 'device.not_found', 'No Device with that id.');
  }

  function connected(device: DeviceSummary): Response | null {
    return device.link.state === 'connected' ? null : failure(409, 'device.not_connected', 'The Device is not connected.', { link: device.link });
  }

  function readLayout(device: string, orientation: string) {
    const found = deviceOr404(device);
    if (found instanceof Response) return found;
    if (!ORIENTATIONS.includes(orientation)) return failure(400, 'request.invalid', 'Unknown orientation.', { part: 'path', reason: orientation });
    const offline = connected(found);
    if (offline) return offline;
    const text = stored.get(slot(device, orientation));
    if (text === undefined) return failure(404, 'layout.not_found', 'The Device stores no Layout for this orientation.');
    const revision = revisionOf(text);
    return json(200, { layout: JSON.parse(text), revision }, { ETag: `"${revision}"` });
  }

  async function pushLayout(request: Request, device: string, orientation: string) {
    const body = await request.text();
    const parsed = parseLayout(body);
    if (!parsed.ok) {
      const issues = parsed.errors.map((e) => ({ rule: e.key, path: e.message.split(/\s/)[0].startsWith('/') ? e.message.split(/\s/)[0] : '', message: e.message }));
      return failure(422, 'layout.invalid', `The Layout has ${issues.length} problems.`, { issues });
    }
    const { layout } = parsed;
    if (layout.orientation !== orientation) return failure(422, 'layout.orientation_mismatch', 'The path and the Layout name different orientations.');
    const found = deviceOr404(device);
    if (found instanceof Response) return found;
    const offline = connected(found);
    if (offline) return offline;
    const holding = active(device);
    if (holding) return failure(409, 'device.busy', 'A Job holds the Device.', { job: holding });
    const expected = request.headers.get('If-Match');
    if (expected !== null) {
      const text = stored.get(slot(device, orientation));
      const current = text === undefined ? null : revisionOf(text);
      const wanted = expected.split(',').map((r) => r.trim().replace(/^"|"$/g, ''));
      const matches = current !== null && (wanted.includes('*') || wanted.includes(current));
      if (!matches) return failure(412, 'layout.changed', "The Device's Layout changed since it was loaded.", { revision: current });
    }
    const text = canonical(layout);
    const bytes = new TextEncoder().encode(text).length;
    const job = startJob('layouts.push', device, { assets_sent: 0, assets_total: 0, bytes_sent: 0, bytes_total: bytes }, () => {
      puts += 1;
      stored.set(slot(device, orientation), text);
      const tiles = layout.pages.reduce((sum, page) => sum + (page.kind === 'grid' ? page.tiles.length : 0), 0);
      const report: PushReport = {
        state: { source: 'device', name: layout.name, orientation: layout.orientation, pages: layout.pages.length, tiles, icons_loaded: 0, icons_missing: 0, bytes, load_ms: 4, error: null },
        bytes,
        chunks: Math.ceil(bytes / 512),
        retransmits: 0,
        elapsed_ms: delayMs,
        assets: { sent: [], unavailable: [] },
        revision: revisionOf(text),
      };
      return report;
    });
    return json(202, job);
  }

  function setLink(id: string, link: LinkState) {
    const current = devices.get(id);
    if (!current) return;
    const next = { ...current, link, rev: current.rev + 1 };
    devices.set(id, next);
    publish({ type: 'device.updated', device: id, data: next });
  }

  return {
    async handle(request: Request, path: string): Promise<Response | null> {
      const method = request.method;
      let match: RegExpExecArray | null;
      if ((match = /^\/devices\/([^/]+)\/layouts\/([^/]+)$/.exec(path))) {
        const [device, orientation] = [decodeURIComponent(match[1]), decodeURIComponent(match[2])];
        if (method === 'GET') return readLayout(device, orientation);
        if (method === 'PUT') return pushLayout(request, device, orientation);
        return failure(405, 'route.method_not_allowed', 'Use GET or PUT.');
      }
      if ((match = /^\/devices\/([^/]+)\/assets\/prune$/.exec(path))) {
        if (method !== 'POST') return failure(405, 'route.method_not_allowed', 'Use POST.');
        const found = deviceOr404(decodeURIComponent(match[1]));
        if (found instanceof Response) return found;
        const offline = connected(found);
        if (offline) return offline;
        const holding = active(found.id);
        if (holding) return failure(409, 'device.busy', 'A Job holds the Device.', { job: holding });
        return json(202, startJob('assets.prune', found.id, { removed: 0, total: 0 }, () => ({ removed: [] })));
      }
      if (path === '/jobs') return method === 'GET' ? json(200, [...jobs.values()]) : failure(405, 'route.method_not_allowed', 'Use GET.');
      if ((match = /^\/jobs\/([^/]+)$/.exec(path))) {
        const job = jobs.get(decodeURIComponent(match[1]));
        if (!job) return failure(404, 'job.not_found', 'No Job with that id.');
        if (method === 'GET') return json(200, job);
        if (method !== 'DELETE') return failure(405, 'route.method_not_allowed', 'Use GET or DELETE.');
        if (!job.cancellable || job.finished_ms !== null) return failure(409, 'job.not_cancellable', "The Job's current phase can't be cancelled.");
        const error = { code: 'job.cancelled', message: 'The Job was cancelled.', details: null };
        return json(200, update(job, { state: 'cancelled', error, cancellable: false, finished_ms: Date.now() }));
      }
      return null;
    },

    async test(request: Request, path: string): Promise<Response> {
      let match: RegExpExecArray | null;
      if ((match = /^\/test\/devices\/([^/]+)\/layouts\/([^/]+)$/.exec(path)) && request.method === 'PUT') {
        stored.set(slot(decodeURIComponent(match[1]), decodeURIComponent(match[2])), JSON.stringify(JSON.parse(await request.text())));
        return json(204, null);
      }
      if ((match = /^\/test\/devices\/([^/]+)\/record$/.exec(path)) && request.method === 'GET') {
        const device = decodeURIComponent(match[1]);
        const read = (o: Orientation) => {
          const text = stored.get(slot(device, o));
          return text === undefined ? null : JSON.parse(text);
        };
        return json(200, { layouts: { landscape: read('landscape'), portrait: read('portrait') }, puts });
      }
      if ((match = /^\/test\/devices\/([^/]+)\/link$/.exec(path)) && request.method === 'POST') {
        const body: unknown = await request.json();
        const state = typeof body === 'object' && body !== null && 'state' in body ? body.state : undefined;
        const link: LinkState =
          state === 'connected' ? { state: 'connected', port: 'COM9', since_ms: Date.now(), protocol: 2 } : { state: 'lost', reason: 'unplugged', detail: '', at_ms: Date.now() };
        setLink(decodeURIComponent(match[1]), link);
        return json(204, null);
      }
      return failure(404, 'route.not_found', 'No test route with that path.');
    },

    jobs: () => [...jobs.values()],
    stored: (device: string, orientation: Orientation) => stored.get(slot(device, orientation)) ?? null,
    store: (device: string, layout: Layout) => void stored.set(slot(device, layout.orientation), canonical(layout)),
    pushed(device: string, layout: StoredLayout) {
      stored.set(slot(device, layout.orientation), JSON.stringify(layout));
      puts += 1;
      minted += 1;
      const job: Job = {
        id: `job_fake${String(minted).padStart(12, '0')}`,
        kind: 'layouts.push',
        scope: { kind: 'device', id: device },
        state: 'succeeded',
        progress: null,
        result: null,
        error: null,
        cancellable: false,
        created_ms: Date.now(),
        finished_ms: Date.now(),
        rev: 1,
      };
      jobs.set(job.id, job);
      publish({ type: 'job.updated', data: job });
    },
    hold: (device: string) => void held.add(device),
    release: (device: string) => {
      releases.get(device)?.();
      releases.delete(device);
    },
    get puts() {
      return puts;
    },
  };
}

export type FakeLayouts = ReturnType<typeof createFakeLayouts>;
