import type { DeviceSummary, Job, SyncData } from './api.ts';
import type { DevicesStore } from './devices.ts';
import type { JobsStore } from './jobs.ts';
import type { Session } from './session.ts';

// The part of a WebSocket the event stream uses, so the fake Companion's socket can stand in.
export interface EventSocket {
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: { code: number }) => void) | null;
  close(code?: number): void;
}

export type OpenSocket = (url: string, protocols: string[]) => EventSocket;

export const openWebSocket: OpenSocket = (url, protocols) => {
  const ws = new WebSocket(url, protocols);
  const socket: EventSocket = { onmessage: null, onclose: null, close: (code) => ws.close(code) };
  ws.onmessage = (event) => socket.onmessage?.(event);
  ws.onclose = (event) => socket.onclose?.(event);
  return socket;
};

// Close codes from docs/companion-api.md, "WebSocket auth".
export const CLOSE_SESSION = 4401;
export const CLOSE_TOO_SLOW = 4008;

// Waits between reconnects. A clean `sync` resets them.
const RETRY_MS = [250, 500, 1000, 2000, 5000];

export interface EventsOptions {
  /** `ws://127.0.0.1:PORT/api/v1/events` */
  url: string;
  session: Session;
  store: DevicesStore;
  /** Where `job.updated` and the `jobs` key of `sync` go. */
  jobs?: JobsStore;
  open?: OpenSocket;
  /** A reconnect met a Companion with a different build, so the page should reload. */
  onNewBuild?: () => void;
  retryMs?: readonly number[];
}

export interface Events {
  close(): void;
}

type Frame = { type?: unknown; build?: unknown; data?: unknown };

function parse(data: unknown): Frame | null {
  if (typeof data !== 'string') return null;
  try {
    const frame: unknown = JSON.parse(data);
    return typeof frame === 'object' && frame !== null ? (frame as Frame) : null;
  } catch {
    return null;
  }
}

/**
 * Follows the Companion's event stream: `hello`, then a `sync` snapshot that
 * replaces the store, then full-object deltas. Reconnects until closed, and
 * asks the session for a new token after close 4401.
 */
export function openEvents(options: EventsOptions): Events {
  const { url, session, store, jobs, open = openWebSocket, onNewBuild, retryMs = RETRY_MS } = options;
  let socket: EventSocket | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let closed = false;
  let failures = 0;
  let build: string | undefined;

  function receive(frame: Frame) {
    switch (frame.type) {
      case 'hello':
        if (typeof frame.build === 'string') {
          if (build !== undefined && frame.build !== build) onNewBuild?.();
          build = frame.build;
        }
        break;
      case 'sync':
        failures = 0;
        store.getState().sync((frame.data as SyncData).devices ?? [], (frame.data as SyncData).jobs ?? []);
        jobs?.getState().sync((frame.data as SyncData).jobs ?? []);
        break;
      case 'device.added':
      case 'device.updated':
        store.getState().upsert(frame.data as DeviceSummary);
        break;
      case 'device.removed':
        store.getState().remove((frame.data as { id: string }).id);
        break;
      case 'job.updated':
        store.getState().job(frame.data as Job);
        jobs?.getState().upsert(frame.data as Job);
        break;
      // Clients ignore event types they don't know.
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
    // A new session or a fresh `sync` after 4008 is worth one immediate try.
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
  };
}
