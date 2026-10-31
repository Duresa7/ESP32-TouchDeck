import { encodeTdi1, TDI1_SIDE } from '../lib/icons/tdi1.ts';
import type { AssetRef, AssetSource, Builtin, CatalogHit, CreateAsset, Job } from './api.ts';
import { RECORDED_BUILTINS, RECORDED_HITS } from './fake-catalog.data.ts';

type Frame = Record<string, unknown> & { type: string };

export interface FakeAssetsOptions {
  publish(frame: Frame): void;
  hits?: readonly CatalogHit[];
  renderMs?: number;
  searchMs?: number;
}

export interface FakeSearch {
  q: string;
  catalog: string | null;
  aborted: boolean;
}

const json = (status: number, body: unknown) =>
  new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

const failure = (status: number, code: string, message: string, details: unknown = null) => json(status, { error: { code, message, details } });

const UPLOAD_MAX = 16 * 1024 * 1024;
const UPLOAD_TYPES: ReadonlySet<string> = new Set(['image/png', 'image/jpeg', 'image/gif']);

function pause(ms: number | Promise<void>, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) return reject(new DOMException('The request was aborted.', 'AbortError'));
    const go = () => {
      signal.removeEventListener('abort', stop);
      resolve();
    };
    const timer = typeof ms === 'number' ? setTimeout(go, ms) : (void ms.then(go), null);
    const stop = () => {
      if (timer !== null) clearTimeout(timer);
      reject(new DOMException('The request was aborted.', 'AbortError'));
    };
    signal.addEventListener('abort', stop, { once: true });
  });
}

async function sha256(bytes: Uint8Array): Promise<string> {
  const copy = new Uint8Array(bytes);
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', copy.buffer));
  return [...digest].map((b) => b.toString(16).padStart(2, '0')).join('');
}

function hashOf(text: string): number {
  let h = 2166136261;
  for (const ch of text) h = Math.imul(h ^ (ch.codePointAt(0) ?? 0), 16777619);
  return h >>> 0;
}

function disc(seed: string, tint: readonly [number, number, number] | null): Uint8Array {
  const h = hashOf(seed);
  const color = tint ?? [96 + (h & 0x7f), 96 + ((h >> 8) & 0x7f), 96 + ((h >> 16) & 0x7f)];
  const ring = 6 + (h % 10);
  const centre = (TDI1_SIDE - 1) / 2;
  return encodeTdi1((x, y) => {
    const d = Math.hypot(x - centre, y - centre);
    const on = d <= 28 && (d >= 28 - ring || d <= 8);
    return [color[0], color[1], color[2], on ? 255 : 0];
  });
}

function rank(hit: CatalogHit, query: string): number | null {
  let best: number | null = null;
  for (const word of [hit.name, ...hit.aliases, ...hit.tags]) {
    const text = word.toLowerCase();
    const r = text === query ? 0 : text.startsWith(query) ? 1 : text.includes(query) ? 2 : null;
    if (r !== null && (best === null || r < best)) best = r;
  }
  return best;
}

export function searchHits(hits: readonly CatalogHit[], q: string, catalog: string | null): CatalogHit[] {
  const query = q.trim().toLowerCase();
  return hits
    .filter((hit) => catalog === null || hit.source === catalog)
    .flatMap((hit, index) => {
      const r = rank(hit, query);
      return r === null ? [] : [{ r, index, hit }];
    })
    .sort((a, b) => a.r - b.r || a.index - b.index)
    .map(({ hit }) => hit);
}

export function createFakeAssets(options: FakeAssetsOptions) {
  const { publish, renderMs = 250, searchMs = 30 } = options;
  let hits = options.hits ?? RECORDED_HITS;
  const files = new Map<string, Uint8Array>();
  const uploads = new Map<string, Uint8Array>();
  const jobs = new Map<string, Job>();
  const created: AssetSource[] = [];
  const searches: FakeSearch[] = [];
  const failing = new Set<string>();
  let offline = false;
  let held: { promise: Promise<void>; release(): void } | null = null;
  let minted = 0;
  let builtins: Promise<Builtin[]> | null = null;

  function listBuiltins(): Promise<Builtin[]> {
    builtins ??= Promise.all(
      RECORDED_BUILTINS.map(async (icon) => {
        const file = disc(`builtin:${icon.name}`, [0xf0, 0xf0, 0xf8]);
        const id = (await sha256(file)).slice(0, 16);
        files.set(id, file);
        return { ...icon, tags: [...icon.tags], id };
      }),
    );
    return builtins;
  }

  function update(job: Job, change: Partial<Job>): Job {
    const next = { ...job, ...change, rev: job.rev + 1 };
    jobs.set(next.id, next);
    publish({ type: 'job.updated', data: next });
    return next;
  }

  async function render(source: AssetSource): Promise<{ file: Uint8Array } | { problem: string }> {
    switch (source.kind) {
      case 'catalog': {
        const hit = hits.find((h) => h.source === source.catalog && h.id === source.id);
        if (!hit) return { problem: 'Catalog icon or variant not found' };
        if (failing.has(source.id)) return { problem: 'Icon download failed: the CDN did not answer' };
        return { file: disc(`catalog:${source.catalog}:${source.id}`, source.tint) };
      }
      case 'gif': {
        const bytes = uploads.get(source.upload);
        uploads.delete(source.upload);
        if (!bytes) return { problem: 'upload.not_found' };
        return { file: new Uint8Array([71,73,70,56,57,97,1,0,1,0,128,0,0,0,0,0,255,255,255,33,255,11,78,69,84,83,67,65,80,69,50,46,48,3,1,0,0,0,33,249,4,4,10,0,0,0,44,0,0,0,0,1,0,1,0,0,2,2,68,1,0,59]) };
      }
      case 'upload': {
        const bytes = uploads.get(source.upload);
        uploads.delete(source.upload);
        if (!bytes) return { problem: 'upload.not_found' };
        return { file: disc(`upload:${await sha256(bytes)}`, null) };
      }
      case 'app':
        return source.path ? { file: disc(`app:${source.path.toLowerCase()}`, null) } : { problem: 'no icon resource in that program' };
      default: {
        const exhaustive: never = source;
        return exhaustive;
      }
    }
  }

  function startRender(body: CreateAsset): Job {
    minted += 1;
    created.push(body.source);
    const job: Job = {
      id: `job_asset${String(minted).padStart(11, '0')}`,
      kind: 'assets.render',
      scope: { kind: 'companion' },
      state: 'running',
      progress: { frames_done: 0, frames_total: 1 },
      result: null,
      error: null,
      cancellable: true,
      created_ms: Date.now(),
      finished_ms: null,
      rev: 1,
    };
    jobs.set(job.id, job);
    publish({ type: 'job.updated', data: job });
    setTimeout(async () => {
      const outcome = await render(body.source);
      const now = jobs.get(job.id);
      if (!now || now.state !== 'running') return;
      if ('problem' in outcome) {
        const code = outcome.problem === 'upload.not_found' ? 'upload.not_found' : 'asset.render_failed';
        const message = code === 'upload.not_found' ? 'The upload expired or never existed.' : `The image or icon couldn't be rendered: ${outcome.problem}`;
        update(now, { state: 'failed', error: { code, message, details: null }, cancellable: false, finished_ms: Date.now() });
        return;
      }
      const id = await sha256(outcome.file);
      files.set(id, outcome.file);
      const gif = body.source.kind === 'gif';
      const result: AssetRef = { id, kind: gif ? 'gif' : 'tdi1', width: gif ? 1 : TDI1_SIDE, height: gif ? 1 : TDI1_SIDE, bytes: outcome.file.length };
      update(now, { state: 'succeeded', progress: { frames_done: 1, frames_total: 1 }, result, cancellable: false, finished_ms: Date.now() });
    }, renderMs);
    return job;
  }

  function isCreateAsset(value: unknown): value is CreateAsset {
    if (typeof value !== 'object' || value === null || !('source' in value)) return false;
    if ('fit' in value && value.fit !== undefined && value.fit !== null && value.fit !== 'contain' && value.fit !== 'cover') return false;
    const source = value.source;
    if (typeof source === 'object' && source !== null && 'kind' in source && source.kind === 'gif') {
      if (!('upload' in source) || typeof source.upload !== 'string' || !('device' in source) || typeof source.device !== 'string' || !('target' in source)) return false;
      const target = source.target;
      return typeof target === 'object' && target !== null && 'width' in target && typeof target.width === 'number' && Number.isInteger(target.width) && target.width > 0 && target.width <= 65535 && 'height' in target && typeof target.height === 'number' && Number.isInteger(target.height) && target.height > 0 && target.height <= 65535 && 'background' in target && Array.isArray(target.background) && target.background.length === 3 && target.background.every((v: unknown) => typeof v === 'number' && Number.isInteger(v) && v >= 0 && v <= 255);
    }
    return typeof source === 'object'  && source !== null && 'kind' in source && ['app', 'catalog', 'upload'].includes(String(source.kind));
  }

  return {
    async handle(request: Request, path: string): Promise<Response | null> {
      const method = request.method;
      const assetRoute = path === '/uploads' || path.startsWith('/assets');
      if (!assetRoute && !/^\/jobs\/job_asset/.test(path)) return null;
      if (offline && assetRoute) throw new TypeError('Failed to fetch');
      if (path === '/assets/catalog/search') {
        if (method !== 'GET') return failure(405, 'route.method_not_allowed', 'Use GET.');
        const url = new URL(request.url);
        const entry: FakeSearch = { q: url.searchParams.get('q') ?? '', catalog: url.searchParams.get('catalog'), aborted: false };
        searches.push(entry);
        try {
          await pause(held?.promise ?? searchMs, request.signal);
        } catch (error) {
          entry.aborted = true;
          throw error;
        }
        return json(200, searchHits(hits, entry.q, entry.catalog));
      }
      if (path === '/assets/builtin') return method === 'GET' ? json(200, await listBuiltins()) : failure(405, 'route.method_not_allowed', 'Use GET.');
      if (path === '/assets/builtin/suggest') {
        if (method !== 'POST') return failure(405, 'route.method_not_allowed', 'Use POST.');
        const body: unknown = await request.json();
        const type = typeof body === 'object' && body !== null && 'action' in body && typeof body.action === 'object' && body.action !== null && 'type' in body.action ? body.action.type : null;
        const name = type === 'consumer' ? 'music' : type === 'launch' ? 'browser' : type === 'command' ? 'terminal' : type === 'open' ? 'folder' : null;
        return json(200, (await listBuiltins()).find((icon) => icon.name === name) ?? null);
      }
      if (path === '/uploads') {
        if (method !== 'POST') return failure(405, 'route.method_not_allowed', 'Use POST.');
        const type = request.headers.get('Content-Type') ?? '';
        if (!UPLOAD_TYPES.has(type)) return failure(415, 'request.media_type', 'Uploads take PNG, JPEG or GIF images.');
        const bytes = new Uint8Array(await request.arrayBuffer());
        if (bytes.length > UPLOAD_MAX) return failure(413, 'request.too_large', 'The image is larger than 16 MiB.');
        const id = `upl_${String(uploads.size + 1).padStart(16, '0')}`;
        uploads.set(id, bytes);
        return json(200, { id, media_type: type, bytes: bytes.length });
      }
      if (path === '/assets') {
        if (method !== 'POST') return failure(405, 'route.method_not_allowed', 'Use POST.');
        const body: unknown = await request.json();
        if (!isCreateAsset(body)) return failure(400, 'request.invalid', 'Expected a source.', { part: 'body', reason: 'source' });
        return json(202, startRender(body));
      }
      const file = /^\/assets\/([^/]+)$/.exec(path);
      if (file) {
        if (method !== 'GET') return failure(405, 'route.method_not_allowed', 'Use GET.');
        await listBuiltins();
        const bytes = files.get(decodeURIComponent(file[1]));
        if (!bytes) return failure(404, 'asset.not_found', 'No Asset with that id.');
        return new Response(new Uint8Array(bytes), { status: 200, headers: { 'Content-Type': bytes[0] === 71 ? 'image/gif' : 'application/vnd.touchdeck.tdi1' } });
      }
      const job = /^\/jobs\/([^/]+)$/.exec(path);
      if (job) {
        const found = jobs.get(decodeURIComponent(job[1]));
        if (!found) return failure(404, 'job.not_found', 'No Job with that id.');
        if (method === 'GET') return json(200, found);
        if (method !== 'DELETE') return failure(405, 'route.method_not_allowed', 'Use GET or DELETE.');
        if (!found.cancellable || found.finished_ms !== null) return failure(409, 'job.not_cancellable', "The Job's current phase can't be cancelled.");
        return json(200, update(found, { state: 'cancelled', error: { code: 'job.cancelled', message: 'The Job was cancelled.', details: null }, cancellable: false, finished_ms: Date.now() }));
      }
      return null;
    },
    jobs: () => [...jobs.values()],
    get created(): readonly AssetSource[] {
      return created;
    },
    get searches(): readonly FakeSearch[] {
      return searches;
    },
    setOffline(on: boolean) {
      offline = on;
    },
    holdSearches(on: boolean) {
      if (on && !held) {
        let release = () => {};
        const promise = new Promise<void>((resolve) => (release = resolve));
        held = { promise, release };
      }
      if (!on && held) {
        held.release();
        held = null;
      }
    },
    failRender(id: string) {
      failing.add(id);
    },
    useHits(next: readonly CatalogHit[]) {
      hits = next;
    },
  };
}

export type FakeAssets = ReturnType<typeof createFakeAssets>;
