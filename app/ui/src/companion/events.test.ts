import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { createActionsStore, type ActionsStore } from './actions.ts';
import type { ApprovalRequest } from './actions-api.ts';
import type { Job } from './api.ts';
import { createDevicesStore, type DevicesStore } from './devices.ts';
import { CLOSE_TOO_SLOW, openEvents, type Events } from './events.ts';
import { createFakeCompanion, type FakeCompanion } from './fake.ts';
import { demoLayout, sampleDevice } from './samples.ts';
import { browserSession, windowSession } from './session.ts';

const URL = 'ws://127.0.0.1:51234/api/v1/events';
const names = (store: DevicesStore) => store.getState().devices.map((d) => d.name);

// Lets the fake's sockets deliver what they queued and the stream react to it. A
// reconnect is a chain of zero-delay tasks, and fake timers push a zero-delay task
// made during a tick to the next millisecond, so this steps a few milliseconds.
async function settle() {
  for (let step = 0; step < 5; step++) await vi.advanceTimersByTimeAsync(1);
}

describe('the devices store', () => {
  test('sync replaces every Device', () => {
    const store = createDevicesStore();
    store.getState().sync([sampleDevice('a', 'Old')]);
    store.getState().sync([sampleDevice('b', 'Desk'), sampleDevice('c', 'Studio')]);
    expect(names(store)).toEqual(['Desk', 'Studio']);
    expect(store.getState().status).toBe('live');
  });

  test('an update with a newer rev replaces the Device in place', () => {
    const store = createDevicesStore();
    store.getState().sync([sampleDevice('a', 'Desk'), sampleDevice('b', 'Studio')]);
    store.getState().upsert({ ...sampleDevice('a', 'Desk 2'), rev: 2 });
    expect(names(store)).toEqual(['Desk 2', 'Studio']);
  });

  test('an older or repeated rev changes nothing', () => {
    const store = createDevicesStore();
    store.getState().sync([{ ...sampleDevice('a', 'Desk'), rev: 5 }]);
    const before = store.getState().devices;
    store.getState().upsert({ ...sampleDevice('a', 'Stale'), rev: 4 });
    store.getState().upsert({ ...sampleDevice('a', 'Same rev'), rev: 5 });
    expect(store.getState().devices).toBe(before);
    expect(names(store)).toEqual(['Desk']);
  });

  test('a succeeded layouts.push Job marks its Device; other Jobs do not', () => {
    const store = createDevicesStore();
    const job = (id: string, finished: number, change: Partial<Job> = {}): Job => ({
      id, kind: 'layouts.push', scope: { kind: 'device', id: 'a' }, state: 'succeeded', progress: null, result: null,
      error: null, cancellable: false, created_ms: 0, finished_ms: finished, rev: 1, ...change,
    });
    store.getState().sync([sampleDevice('a', 'Desk')], [job('job_2', 20), job('job_1', 10), job('job_3', 30, { state: 'failed' })]);
    expect(store.getState().layoutPushes).toEqual({ a: 'job_2' });
    store.getState().job(job('job_4', 40, { state: 'running' }));
    store.getState().job(job('job_5', 50, { kind: 'assets.prune' }));
    store.getState().job(job('job_6', 60, { scope: { kind: 'companion' } }));
    expect(store.getState().layoutPushes).toEqual({ a: 'job_2' });
    store.getState().job(job('job_7', 70));
    expect(store.getState().layoutPushes).toEqual({ a: 'job_7' });
  });

  test('an added Device goes last and a removed one goes away', () => {
    const store = createDevicesStore();
    store.getState().sync([sampleDevice('a', 'Desk')]);
    store.getState().upsert(sampleDevice('b', 'Studio'));
    store.getState().remove('a');
    expect(names(store)).toEqual(['Studio']);
  });
});

describe('the event stream', () => {
  let fake: FakeCompanion;
  let store: DevicesStore;
  let events: Events | undefined;

  beforeEach(() => {
    vi.useFakeTimers();
    fake = createFakeCompanion({ devices: [sampleDevice('mock0001', 'Desk'), sampleDevice('mock0002', 'Studio')], system: { build: 'one' } });
    store = createDevicesStore();
  });

  afterEach(() => {
    events?.close();
    vi.useRealTimers();
  });

  const follow = (options: Partial<Parameters<typeof openEvents>[0]> = {}) => {
    events = openEvents({ url: URL, session: windowSession(fake.bridge), store, open: fake.openSocket, ...options });
  };

  test('sync fills the store, then deltas keep it current', async () => {
    follow();
    expect(store.getState().status).toBe('connecting');
    await settle();
    expect(store.getState().status).toBe('live');
    expect(names(store)).toEqual(['Desk', 'Studio']);

    fake.update('mock0002', { name: 'Studio B' });
    fake.add(sampleDevice('mock0003', 'Stream'));
    fake.remove('mock0001');
    fake.publish({ type: 'integration.status', data: {} });
    await settle();
    expect(names(store)).toEqual(['Studio B', 'Stream']);
  });

  test('job.updated reaches the store', async () => {
    follow();
    await settle();
    fake.setLayout('mock0002', demoLayout());
    await settle();
    expect(Object.keys(store.getState().layoutPushes)).toEqual(['mock0002']);
  });

  test('a delta older than the store is ignored', async () => {
    follow();
    await settle();
    fake.update('mock0001', { name: 'Desk 2' });
    fake.publish({ type: 'device.updated', device: 'mock0001', data: { ...sampleDevice('mock0001', 'Late'), rev: 1 } });
    await settle();
    expect(names(store)).toEqual(['Desk 2', 'Studio']);
  });

  test('after a drop it reconnects and the new sync replaces the state', async () => {
    follow({ retryMs: [100] });
    await settle();
    fake.dropSockets(1001);
    fake.remove('mock0001');
    await settle();
    expect(store.getState().status).toBe('reconnecting');
    expect(names(store)).toEqual(['Desk', 'Studio']);
    await vi.advanceTimersByTimeAsync(100);
    await settle();
    expect(store.getState().status).toBe('live');
    expect(names(store)).toEqual(['Studio']);
  });

  test('close 4008 reconnects at once for a fresh sync', async () => {
    follow({ retryMs: [10_000] });
    await settle();
    fake.dropSockets(CLOSE_TOO_SLOW);
    await settle();
    expect(store.getState().status).toBe('live');
    expect(fake.openSockets).toBe(1);
  });

  test('close 4401 gets a new session from Electron and reconnects', async () => {
    follow();
    await settle();
    fake.revokeSessions();
    await settle();
    expect(fake.minted).toBe(2);
    expect(fake.openSockets).toBe(1);
    expect(store.getState().status).toBe('live');
  });

  test('a browser without a session stops and says it has to pair', async () => {
    events = openEvents({ url: URL, session: browserSession({ getItem: () => null, removeItem: () => {} }), store, open: fake.openSocket });
    await settle();
    expect(store.getState().status).toBe('unpaired');
    expect(fake.openSockets).toBe(0);
  });

  test('a different build after a reconnect asks for a reload', async () => {
    const onNewBuild = vi.fn();
    follow({ onNewBuild, retryMs: [0] });
    await settle();
    expect(onNewBuild).not.toHaveBeenCalled();
    fake.publish({ type: 'hello', api: 'v1', build: 'two' });
    await settle();
    expect(onNewBuild).toHaveBeenCalledOnce();
  });

  test('approvals, the Action log and the key mirror reach the actions store', async () => {
    const actions: ActionsStore = createActionsStore();
    follow({ actions });
    await settle();
    const request: ApprovalRequest = {
      id: 'apr_1', device: 'mock0001', page: 'p', tile: 't', hash: 'h', kind: 'command', program: 'robocopy', args: [], cwd: '', shell: false, created_ms: 0,
    };
    fake.publish({ type: 'approval.requested', data: request });
    fake.publish({ type: 'approval.requested', data: { id: 'apr_2' } });
    await settle();
    expect(actions.getState().approvals).toEqual([request]);

    fake.publish({ type: 'approval.resolved', data: { id: 'apr_1', answer: 'always' } });
    fake.publish({ type: 'action.logged', data: { id: 'log_1', device: 'mock0001', at_ms: 1, page: 'p', tile: 't', kind: 'command', status: 'started', detail: '', pid: 4101, exit_code: null } });
    await settle();
    expect(actions.getState()).toMatchObject({ approvals: [], allowedRev: 1, log: { mock0001: [{ id: 'log_1', status: 'started' }] } });

    const emit = () => fake.fetch('http://127.0.0.1:51234/api/v1/test/devices/mock0001/emit', { method: 'POST', body: JSON.stringify({ events: [{ m: 'key.report', type: 'kbd', mods: 0, keys: [4] }] }) });
    await emit();
    await settle();
    expect(actions.getState().mirror.mock0001).toBeUndefined();
    const unwatch = actions.getState().watchKeys('mock0001');
    await settle();
    await emit();
    await settle();
    expect(actions.getState().mirror.mock0001).toMatchObject([{ report: 'kbd 0 [4]', outcome: 'typed' }]);
    unwatch();
    await settle();
    await emit();
    await settle();
    expect(actions.getState().mirror.mock0001).toHaveLength(1);
  });

  test('closing stops the stream for good', async () => {
    follow({ retryMs: [0] });
    await settle();
    events!.close();
    await vi.advanceTimersByTimeAsync(50);
    expect(fake.openSockets).toBe(0);
  });
});
