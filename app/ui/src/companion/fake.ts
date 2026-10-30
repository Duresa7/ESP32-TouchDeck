import type { DeviceSummary, Layout, SystemInfo } from './api.ts';
import { CLOSE_SESSION, type EventSocket } from './events.ts';
import { createFakeActions } from './fake-actions.ts';
import { createFakeConsole } from './fake-console.ts';
import { createFakeLayouts } from './fake-layouts.ts';
import { createFakeSessions, type FakeBrowser } from './fake-sessions.ts';
import { fakeThisPc } from './fake-this-pc.ts';
import { createFakeStartup } from './fake-startup.ts';
import type { OpenTouchDeckBridge } from './session.ts';

// An in-memory Companion for tests and the demo mode of `pnpm dev:ui`. It answers
// `system.ping`, `system.get`, `devices.list`, `devices.get` and `layouts.get` the way
// docs/companion-api.md says, checks session tokens, and runs the event stream:
// `hello`, `sync`, then `device.added`, `device.updated`, `device.removed` and `job.updated`. fake-layouts.ts
// answers the Layouts, Jobs and Assets prune routes and sends `job.updated`. fake-sessions.ts,
// fake-startup.ts and fake-console.ts answer the session, start at sign-in and console routes.

export interface FakeCompanionOptions {
  devices?: DeviceSummary[];
  /** The Layout each Device stores at the start, by Device id. */
  layouts?: Record<string, Layout>;
  system?: Partial<SystemInfo>;
  /** How long a fake Job runs. */
  jobMs?: number;
  /** Browsers paired before the start. */
  browsers?: readonly FakeBrowser[];
  startAtSignIn?: boolean;
}

type Frame = Record<string, unknown> & { type: string };

const topicKey = (topic: unknown, device: unknown) => `${String(topic)}/${String(device)}`;

const TOPIC_OF_FRAME: ReadonlyMap<string, string> = new Map([
  ['device.console', 'console'],
  ['device.key-mirror', 'key-mirror'],
]);

class FakeSocket implements EventSocket {
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number }) => void) | null = null;
  private seq = 0;
  private closing = false;
  private readonly sockets: Set<FakeSocket>;
  readonly token: string;
  readonly topics = new Set<string>();

  constructor(sockets: Set<FakeSocket>, token: string) {
    this.sockets = sockets;
    this.token = token;
  }

  // Frames arrive on a later task, in order, and frames pushed before a close still arrive.
  push(frame: Frame) {
    if (this.closing) return;
    const topic = TOPIC_OF_FRAME.get(frame.type);
    if (topic !== undefined && !this.topics.has(topicKey(topic, frame.device))) return;
    const data = JSON.stringify({ ...frame, seq: this.seq++ });
    setTimeout(() => this.onmessage?.({ data }), 0);
  }

  send(text: string) {
    let request: unknown;
    try {
      request = JSON.parse(text);
    } catch {
      return;
    }
    if (typeof request !== 'object' || request === null || !('op' in request) || !('topic' in request) || !('device' in request)) return;
    if (request.topic === 'state') return;
    const key = topicKey(request.topic, request.device);
    if (request.op === 'subscribe') this.topics.add(key);
    if (request.op === 'unsubscribe') this.topics.delete(key);
  }

  close(code = 1000) {
    if (this.closing) return;
    this.closing = true;
    this.sockets.delete(this);
    setTimeout(() => {
      this.onmessage = null;
      this.onclose?.({ code });
    }, 0);
  }
}

const json = (status: number, body: unknown) =>
  new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

const failure = (status: number, code: string, message: string, details: unknown = null) =>
  json(status, { error: { code, message, details } });

export function createFakeCompanion(options: FakeCompanionOptions = {}) {
  const devices = new Map((options.devices ?? []).map((d) => [d.id, structuredClone(d)]));
  const system: SystemInfo = { version: '0.2.0', build: 'fake', api: 'v1', mock: true, data_dir_override: false, ...options.system };
  const sockets = new Set<FakeSocket>();
  let minted = 0;
  let requests = 0;
  const layouts = createFakeLayouts({ devices, publish, seed: options.layouts, delayMs: options.jobMs });
  const actions = createFakeActions({ devices, publish, stored: layouts.stored });
  const sessions = createFakeSessions({ browsers: options.browsers, onRevoke: signOut });
  const startup = createFakeStartup(options.startAtSignIn);
  const consoles = createFakeConsole({ devices, publish });

  function mint() {
    minted += 1;
    return sessions.mint({ kind: 'window', label: 'Window' });
  }

  function publish(frame: Frame) {
    for (const socket of sockets) socket.push(frame);
  }

  function signOut(tokens: ReadonlySet<string>) {
    for (const socket of [...sockets].filter((s) => tokens.has(s.token))) {
      socket.push({ type: 'auth.revoked', data: {} });
      socket.close(CLOSE_SESSION);
    }
  }

  // What Electron's preload would hand the window: a fresh session on every ask.
  // Main fires onSessionChanged only after it mints one on its own, which the fake never does.
  const bridge: OpenTouchDeckBridge = {
    version: 1,
    session: () => Promise.resolve(mint()),
    onSessionChanged: () => {},
  };

  async function fetch(input: string | URL | Request, init?: RequestInit): Promise<Response> {
    requests += 1;
    const request = new Request(input, init);
    const url = new URL(request.url);
    if (!url.pathname.startsWith('/api/v1/')) return failure(404, 'route.not_found', 'No route with that path.');
    const path = url.pathname.slice('/api/v1'.length);
    const get = request.method === 'GET';

    if (path === '/system/ping') return get ? json(200, { api: ['v1'] }) : failure(405, 'route.method_not_allowed', 'Use GET.');
    if (path.startsWith('/test/')) return (await actions.test(request, path)) ?? layouts.test(request, path);

    const auth = request.headers.get('Authorization') ?? '';
    const token = auth.startsWith('Bearer ') ? auth.slice('Bearer '.length) : '';
    if (!token) return failure(401, 'auth.missing', 'No session token.');
    const status = sessions.status(token);
    if (status === 'revoked') return failure(401, 'auth.revoked', 'The session was revoked.');
    if (status === 'unknown') return failure(401, 'auth.invalid', 'The session token is unknown.');

    const routed = sessions.handle(request, path, token) ?? (await startup.handle(request, path)) ?? (await consoles.handle(request, path));
    if (routed) return routed;
    if (get && path === '/integrations') return json(200, [fakeThisPc().summary]);
    if (get && path === '/integrations/this_pc') return json(200, fakeThisPc());
    const device = /^\/devices\/([^/]+)$/.exec(path);
    if (path === '/system' || path === '/devices' || device) {
      if (!get) return failure(405, 'route.method_not_allowed', 'Use GET.');
      if (path === '/system') return json(200, system);
      if (path === '/devices') return json(200, [...devices.values()]);
      const found = devices.get(decodeURIComponent(device![1]));
      return found ? json(200, found) : failure(404, 'device.not_found', 'No Device with that id.');
    }
    return (await layouts.handle(request, path)) ?? actions.handle(request, path) ?? failure(404, 'route.not_found', 'No route with that path.');
  }

  function openSocket(url: string, protocols: string[]): EventSocket {
    const bearer = protocols.find((p) => p.startsWith('touchdeck.bearer.'));
    const token = bearer?.slice('touchdeck.bearer.'.length) ?? '';
    const socket = new FakeSocket(sockets, token);
    if (!new URL(url).pathname.endsWith('/api/v1/events') || !protocols.includes('touchdeck.v1') || sessions.status(token) !== 'live') {
      socket.close(CLOSE_SESSION);
      return socket;
    }
    sockets.add(socket);
    socket.push({ type: 'hello', api: 'v1', build: system.build, session: { id: 'fake-session', kind: 'window' } });
    socket.push({ type: 'sync', data: { devices: [...devices.values()], jobs: layouts.jobs(), approvals: actions.pending(), integrations: [fakeThisPc().summary] } });
    return socket;
  }

  return {
    fetch,
    openSocket,
    bridge,
    layouts,
    actions,
    sessions,
    startup,
    consoles,
    /** Sessions minted so far through the bridge. */
    get minted() {
      return minted;
    },
    /** HTTP requests answered so far. */
    get requests() {
      return requests;
    },
    get openSockets() {
      return sockets.size;
    },
    /** Each open socket's opt-in topics, as `topic/device`. */
    get topics() {
      return [...sockets].map((socket) => [...socket.topics]);
    },
    /** A Device appears: `device.added`. */
    add(device: DeviceSummary) {
      devices.set(device.id, structuredClone(device));
      publish({ type: 'device.added', device: device.id, data: device });
    },
    /** A Device changes: its `rev` goes up and `device.updated` carries the whole object. */
    update(id: string, change: Partial<Omit<DeviceSummary, 'id' | 'rev'>>) {
      const current = devices.get(id);
      if (!current) throw new Error(`no fake Device ${id}`);
      const next = { ...current, ...change, rev: current.rev + 1 };
      devices.set(id, next);
      publish({ type: 'device.updated', device: id, data: next });
      return next;
    },
    /** A `layouts.push` Job puts `layout` on a Device and succeeds: `job.updated`. */
    setLayout(id: string, layout: Layout) {
      layouts.pushed(id, layout);
    },
    /** A Device is forgotten: `device.removed`. */
    remove(id: string) {
      devices.delete(id);
      publish({ type: 'device.removed', data: { id } });
    },
    /** Reset pairing: every session ends, sockets get `auth.revoked` and close 4401. */
    revokeSessions() {
      sessions.revokeAll();
    },
    /** Drops every socket with `code`, as a restart (1001) or a slow client (4008) would. */
    dropSockets(code: number) {
      for (const socket of [...sockets]) socket.close(code);
    },
    /** Sends a frame as is, such as an event type this client doesn't know. */
    publish,
  };
}

export type FakeCompanion = ReturnType<typeof createFakeCompanion>;
