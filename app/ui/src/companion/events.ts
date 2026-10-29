import type { ActionsStore } from './actions.ts';
import type { ActionLog, Answer, ApprovalRequest, ApprovalResolved, DeviceConsole, DeviceKeyMirror, DeviceRemoved, DeviceSummary, IntegrationSummary, Job, SyncData, Topic } from './api.ts';
import type { DevicesStore } from './devices.ts';
import type { IntegrationsStore } from './integrations.ts';
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

type OptInTopic = Exclude<Topic, 'state'>;

const RETRY_MS = [250, 500, 1000, 2000, 5000];

export interface EventsOptions {
  url: string;
  session: Session;
  store: DevicesStore;
  jobs?: JobsStore;
  actions?: ActionsStore;
  integrations?: IntegrationsStore;
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
  | { type: 'device.console'; data: DeviceConsole }
  | { type: 'integration.status'; data: IntegrationSummary }
  | { type: 'approval.requested' | 'approval.resolved' | 'action.logged' | 'device.key-mirror'; data: unknown };

const HANDLED: ReadonlySet<unknown> = new Set<Frame['type']>([
  'hello',
  'sync',
  'device.added',
  'device.updated',
  'device.removed',
  'job.updated',
  'device.console',
  'approval.requested',
  'approval.resolved',
  'action.logged',
  'device.key-mirror',
  'integration.status',
]);

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null && !Array.isArray(value);
const isStrings = (value: unknown): value is string[] => Array.isArray(value) && value.every((item) => typeof item === 'string');
const isNumberOrNull = (value: unknown): value is number | null => value === null || typeof value === 'number';

function isApprovalRequest(value: unknown): value is ApprovalRequest {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.device === 'string' &&
    typeof value.page === 'string' &&
    typeof value.tile === 'string' &&
    typeof value.hash === 'string' &&
    typeof value.created_ms === 'number' &&
    typeof value.kind === 'string' &&
    typeof value.program === 'string' &&
    isStrings(value.args) &&
    typeof value.cwd === 'string' &&
    typeof value.shell === 'boolean'
  );
}

const ANSWERS = { once: true, always: true, deny: true, expired: true } satisfies Record<Answer, true>;

function isApprovalResolved(value: unknown): value is ApprovalResolved {
  return isRecord(value) && typeof value.id === 'string' && typeof value.answer === 'string' && Object.hasOwn(ANSWERS, value.answer);
}

function isActionLog(value: unknown): value is ActionLog {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.device === 'string' &&
    typeof value.at_ms === 'number' &&
    typeof value.page === 'string' &&
    typeof value.tile === 'string' &&
    typeof value.kind === 'string' &&
    typeof value.status === 'string' &&
    typeof value.detail === 'string' &&
    isNumberOrNull(value.pid) &&
    isNumberOrNull(value.exit_code)
  );
}

function isKeyMirror(value: unknown): value is DeviceKeyMirror {
  if (!isRecord(value)) return false;
  const gap = value.gap;
  return (
    typeof value.device === 'string' &&
    typeof value.seq === 'number' &&
    typeof value.report === 'string' &&
    typeof value.outcome === 'string' &&
    (gap === null || (Array.isArray(gap) && gap.length === 2 && gap.every((n) => typeof n === 'number'))) &&
    isStrings(value.errors) &&
    typeof value.arrived_ms === 'number'
  );
}

const syncedApprovals = (approvals: unknown): ApprovalRequest[] => (Array.isArray(approvals) ? approvals.filter(isApprovalRequest) : []);

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
  const { url, session, store, jobs, actions, integrations, open = openWebSocket, onNewBuild, retryMs = RETRY_MS } = options;
  let socket: EventSocket | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let closed = false;
  let failures = 0;
  let build: string | undefined;
  let greeted: EventSocket | null = null;
  const consoles = new Map<string, Set<(line: string) => void>>();
  const topics = new Map<string, { topic: OptInTopic; device: string }>();

  function ask(op: 'subscribe' | 'unsubscribe', topic: OptInTopic, device: string) {
    if (socket && socket === greeted) socket.send(JSON.stringify({ op, topic, device }));
  }

  function openTopic(topic: OptInTopic, device: string) {
    topics.set(`${topic}/${device}`, { topic, device });
    ask('subscribe', topic, device);
  }

  function closeTopic(topic: OptInTopic, device: string) {
    if (topics.delete(`${topic}/${device}`)) ask('unsubscribe', topic, device);
  }

  function receive(frame: Frame) {
    switch (frame.type) {
      case 'hello':
        if (typeof frame.build === 'string') {
          if (build !== undefined && frame.build !== build) onNewBuild?.();
          build = frame.build;
        }
        greeted = socket;
        for (const { topic, device } of topics.values()) ask('subscribe', topic, device);
        break;
      case 'sync':
        failures = 0;
        store.getState().sync(frame.data.devices ?? [], frame.data.jobs ?? []);
        jobs?.getState().sync(frame.data.jobs ?? []);
        actions?.getState().syncApprovals(syncedApprovals(frame.data.approvals));
        integrations?.getState().sync(frame.data.integrations ?? []);
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
      case 'integration.status':
        integrations?.getState().upsert(frame.data);
        break;
      default: {
        const _exhaustive: never = frame;
        void _exhaustive;
      }
    }
  }

  function mirrorKeys(watching: Record<string, number>, before: Record<string, number> = {}) {
    for (const device of Object.keys(watching)) if (!Object.hasOwn(before, device)) openTopic('key-mirror', device);
    for (const device of Object.keys(before)) if (!Object.hasOwn(watching, device)) closeTopic('key-mirror', device);
  }

  mirrorKeys(actions?.getState().watching ?? {});
  const unwatch = actions?.subscribe((state, before) => {
    if (state.watching !== before.watching) mirrorKeys(state.watching, before.watching);
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
    console(device, onLine) {
      let listeners = consoles.get(device);
      if (!listeners) {
        listeners = new Set();
        consoles.set(device, listeners);
        openTopic('console', device);
      }
      listeners.add(onLine);
      const own = listeners;
      return () => {
        if (!own.delete(onLine) || own.size > 0 || consoles.get(device) !== own) return;
        consoles.delete(device);
        closeTopic('console', device);
      };
    },
  };
}
