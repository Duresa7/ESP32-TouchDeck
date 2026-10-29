import type { DeviceConsole, DeviceRemoved, DeviceSummary, Job, SyncData } from './api.ts';
import type { DevicesStore } from './devices.ts';
import type { JobsStore } from './jobs.ts';
import type { Session } from './session.ts';

export interface EventSocket {
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: { code: number }) => void) | null;
  send(data: string): void;
  close(code?: number): void;
}

export type OpenSocket = (url: string, protocols: string[]) => EventSocket;

export const openWebSocket: OpenSocket = (url, protocols) => {
  const ws = new WebSocket(url, protocols);
  const socket: EventSocket = {
    onmessage: null,
    onclose: null,
    send: (data) => {
      if (ws.readyState === WebSocket.OPEN) ws.send(data);
    },
    close: (code) => ws.close(code),
  };
  ws.onmessage = (event) => socket.onmessage?.(event);
  ws.onclose = (event) => socket.onclose?.(event);
  return socket;
};

export const CLOSE_SESSION = 4401;
export const CLOSE_TOO_SLOW = 4008;

const RETRY_MS = [250, 500, 1000, 2000, 5000];

export interface EventsOptions {
  url: string;
  session: Session;
  store: DevicesStore;
  jobs?: JobsStore;
  open?: OpenSocket;
  onNewBuild?: () => void;
  retryMs?: readonly number[];
}

export interface Events {
  close(): void;
  /** Opts in to one Device's `console` topic while `onLine` listens. The returned function stops listening. */
  console(device: string, onLine: (line: string) => void): () => void;
}

type Frame =
  | { type: 'hello'; build?: unknown }
  | { type: 'sync'; data: Partial<SyncData> }
  | { type: 'device.added' | 'device.updated'; data: DeviceSummary }
  | { type: 'device.removed'; data: DeviceRemoved }
  | { type: 'job.updated'; data: Job }
  | { type: 'device.console'; data: DeviceConsole };

const HANDLED: ReadonlySet<unknown> = new Set<Frame['type']>(['hello', 'sync', 'device.added', 'device.updated', 'device.removed', 'job.updated', 'device.console']);

function parse(data: unknown): Frame | null {
  if (typeof data !== 'string') return null;
  let frame: unknown;
  try {
    frame = JSON.parse(data);
  } catch {
    return null;
  }
  if (typeof frame !== 'object' || frame === null || !('type' in frame) || !HANDLED.has(frame.type)) return null;
  return frame as Frame;
}

export function openEvents(options: EventsOptions): Events {
  const { url, session, store, jobs, open = openWebSocket, onNewBuild, retryMs = RETRY_MS } = options;
  let socket: EventSocket | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let closed = false;
  let failures = 0;
  let build: string | undefined;
  let greeted: EventSocket | null = null;
  const consoles = new Map<string, Set<(line: string) => void>>();

  function topic(op: 'subscribe' | 'unsubscribe', device: string) {
    if (socket && socket === greeted) socket.send(JSON.stringify({ op, topic: 'console', device }));
  }

  function receive(frame: Frame) {
    switch (frame.type) {
      case 'hello':
        if (typeof frame.build === 'string') {
          if (build !== undefined && frame.build !== build) onNewBuild?.();
          build = frame.build;
        }
        greeted = socket;
        for (const device of consoles.keys()) topic('subscribe', device);
        break;
      case 'sync':
        failures = 0;
        store.getState().sync(frame.data.devices ?? [], frame.data.jobs ?? []);
        jobs?.getState().sync(frame.data.jobs ?? []);
        break;
      case 'device.added':
      case 'device.updated':
        store.getState().upsert(frame.data);
        break;
      case 'device.removed':
        store.getState().remove(frame.data.id);
        break;
      case 'job.updated':
        store.getState().job(frame.data);
        jobs?.getState().upsert(frame.data);
        break;
      case 'device.console':
        for (const onLine of consoles.get(frame.data.device) ?? []) onLine(frame.data.line);
        break;
      default: {
        const _exhaustive: never = frame;
        void _exhaustive;
      }
    }
  }

  function retry(delay: number) {
    store.getState().setStatus('reconnecting');
    timer = setTimeout(() => void connect(), delay);
  }

  function backoff() {
    failures += 1;
    retry(retryMs[Math.min(failures - 1, retryMs.length - 1)]);
  }

  async function connect() {
    let token: string | null;
    try {
      token = await session.token();
    } catch {
      if (!closed) backoff();
      return;
    }
    if (closed) return;
    if (token === null) {
      store.getState().setStatus('unpaired');
      return;
    }
    const current = open(url, ['touchdeck.v1', `touchdeck.bearer.${token}`]);
    socket = current;
    current.onmessage = (event) => {
      const frame = parse(event.data);
      if (frame) receive(frame);
    };
    current.onclose = (event) => {
      if (closed || socket !== current) return;
      socket = null;
      void reconnect(event.code, token);
    };
  }

  async function reconnect(code: number, token: string) {
    const immediate = failures === 0 && (code === CLOSE_SESSION || code === CLOSE_TOO_SLOW);
    if (code === CLOSE_SESSION) {
      let fresh: string | null;
      try {
        fresh = await session.renew(token);
      } catch {
        if (!closed) backoff();
        return;
      }
      if (closed) return;
      if (fresh === null) {
        store.getState().setStatus('unpaired');
        return;
      }
    }
    if (immediate) {
      failures += 1;
      retry(0);
    } else {
      backoff();
    }
  }

  void connect();

  return {
    close() {
      closed = true;
      clearTimeout(timer);
      socket?.close(1000);
      socket = null;
    },
    console(device, onLine) {
      let listeners = consoles.get(device);
      if (!listeners) {
        listeners = new Set();
        consoles.set(device, listeners);
        topic('subscribe', device);
      }
      listeners.add(onLine);
      const own = listeners;
      return () => {
        if (!own.delete(onLine) || own.size > 0 || consoles.get(device) !== own) return;
        consoles.delete(device);
        topic('unsubscribe', device);
      };
    },
  };
}
