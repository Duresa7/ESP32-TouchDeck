import type { DeviceRemoved, DeviceSummary, Job, SyncData } from './api.ts';
import { isActionLog, isApprovalRequest, isApprovalResolved, isKeyMirror, syncedApprovals } from './actions-api.ts';
import type { ActionsStore } from './actions.ts';
import type { DevicesStore } from './devices.ts';
import type { JobsStore } from './jobs.ts';
import type { Session } from './session.ts';

export interface EventSocket {
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: { code: number }) => void) | null;
  /** Sends a `subscribe` or `unsubscribe` op. */
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
  /** Where approvals, the Action log and the key mirror go, and which Devices' key mirror to ask for. */
  actions?: ActionsStore;
  open?: OpenSocket;
  onNewBuild?: () => void;
  retryMs?: readonly number[];
}

export interface Events {
  close(): void;
}

type Frame =
  | { type: 'hello'; build?: unknown }
  | { type: 'sync'; data: Partial<SyncData> }
  | { type: 'device.added' | 'device.updated'; data: DeviceSummary }
  | { type: 'device.removed'; data: DeviceRemoved }
  | { type: 'job.updated'; data: Job }
  | { type: 'approval.requested' | 'approval.resolved' | 'action.logged' | 'device.key-mirror'; data: unknown };

const HANDLED: ReadonlySet<unknown> = new Set<Frame['type']>([
  'hello',
  'sync',
  'device.added',
  'device.updated',
  'device.removed',
  'job.updated',
  'approval.requested',
  'approval.resolved',
  'action.logged',
  'device.key-mirror',
]);

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
  const { url, session, store, jobs, actions, open = openWebSocket, onNewBuild, retryMs = RETRY_MS } = options;
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
        store.getState().sync(frame.data.devices ?? [], frame.data.jobs ?? []);
        jobs?.getState().sync(frame.data.jobs ?? []);
        actions?.getState().syncApprovals(syncedApprovals(frame.data));
        mirrorTopics(socket, watched(), []);
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
      case 'approval.requested':
        if (isApprovalRequest(frame.data)) actions?.getState().requested(frame.data);
        break;
      case 'approval.resolved':
        if (isApprovalResolved(frame.data)) actions?.getState().answered(frame.data);
        break;
      case 'action.logged':
        if (isActionLog(frame.data)) actions?.getState().logged(frame.data);
        break;
      case 'device.key-mirror':
        if (isKeyMirror(frame.data)) actions?.getState().mirrored(frame.data);
        break;
      default: {
        const _exhaustive: never = frame;
        void _exhaustive;
      }
    }
  }

  const watched = () => Object.keys(actions?.getState().watching ?? {});

  function mirrorTopics(to: EventSocket | null, on: string[], off: string[]) {
    for (const [op, devices] of [['subscribe', on], ['unsubscribe', off]] as const) {
      for (const device of devices) to?.send(JSON.stringify({ op, topic: 'key-mirror', device }));
    }
  }

  const unwatch = actions?.subscribe((state, before) => {
    if (state.watching === before.watching) return;
    const now = Object.keys(state.watching);
    const was = Object.keys(before.watching);
    mirrorTopics(socket, now.filter((d) => !was.includes(d)), was.filter((d) => !now.includes(d)));
  });

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
      unwatch?.();
      clearTimeout(timer);
      socket?.close(1000);
      socket = null;
    },
  };
}
