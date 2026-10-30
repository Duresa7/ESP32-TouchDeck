import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { createClient, type Companion } from '../../companion/client.ts';
import { createDevicesStore } from '../../companion/devices.ts';
import { openEvents, type Events } from '../../companion/events.ts';
import { createFakeCompanion, type FakeCompanion } from '../../companion/fake.ts';
import { createIntegrationsStore, type IntegrationsStore } from '../../companion/integrations.ts';
import { windowSession } from '../../companion/session.ts';
import { SecretField } from '../../screens/integrations/SecretField.tsx';
import { createDetailStore, REFRESH_FLOOR_MS, type DetailStore } from './detail.ts';

const BASE = 'http://127.0.0.1:51234';
const SECRET = 'spec-secret-7f3a91';

let fake: FakeCompanion;
let companion: Companion;
let integrations: IntegrationsStore;
let events: Events;
let requests: { method: string; url: string; body: string }[];
let clock: number;

async function settle() {
  for (let i = 0; i < 5; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

async function detail(): Promise<DetailStore> {
  const store = createDetailStore('cliproxyapi', { companion, now: () => clock });
  await store.load();
  return store;
}

async function setUp(store: DetailStore, url = 'https://hub.example.com') {
  store.setValue('enabled', true);
  store.setValue('url', url);
  await store.save();
  expect(await store.putSecret({ field: 'management_key', value: SECRET })).toBe(true);
}

beforeEach(() => {
  clock = Date.UTC(2026, 9, 29, 12);
  fake = createFakeCompanion({ integrations: { now: () => clock } });
  requests = [];
  vi.stubGlobal('fetch', async (input: string | URL | Request, init?: RequestInit) => {
    const request = new Request(input, init);
    requests.push({ method: request.method, url: request.url, body: await request.clone().text() });
    return fake.fetch(request);
  });
  companion = createClient(BASE, windowSession(fake.bridge));
  integrations = createIntegrationsStore();
  events = openEvents({ url: 'ws://127.0.0.1:51234/api/v1/events', session: windowSession(fake.bridge), store: createDevicesStore(), integrations, open: fake.openSocket });
});

afterEach(() => {
  events.close();
  vi.unstubAllGlobals();
});

describe('the Integration view', () => {
  it('starts from the sync snapshot and follows integration.status', async () => {
    await settle();
    expect(integrations.getState().integrations.map((i) => [i.id, i.status.state])).toEqual([['cliproxyapi', 'not_configured'], ['this_pc', 'ok']]);
    await setUp(await detail());
    await settle();
    expect(integrations.getState().integrations[0].status.state).toBe('ok');
    expect(integrations.getState().integrations[0].enabled).toBe(true);
  });

  it('fills the form from the config and knows when it has changes', async () => {
    const store = await detail();
    expect(store.getState().values).toEqual({ enabled: false, url: 'http://127.0.0.1:8317', interval_s: '300' });
    expect(store.getState().secrets).toEqual({ management_key: { set: false, hint: null } });
    expect(store.dirty()).toBe(false);
    store.setValue('interval_s', '120');
    expect(store.dirty()).toBe(true);
    await store.save();
    expect(store.getState().notice?.event).toBe('saved');
    expect(store.dirty()).toBe(false);
  });

  it('shows what the Companion normalized after a save', async () => {
    const store = await detail();
    store.setValue('url', '  https://hub.example.com/  ');
    store.setValue('interval_s', '5');
    await store.save();
    expect(store.getState().values).toMatchObject({ url: 'https://hub.example.com', interval_s: '60' });
  });

  it('refuses an invalid field inline without asking the Companion', async () => {
    const store = await detail();
    const before = requests.length;
    store.setValue('interval_s', 'soon');
    await store.save();
    expect(store.getState().errors).toEqual({ interval_s: 'Enter a whole number.' });
    expect(requests.length).toBe(before);
    store.setValue('interval_s', '90');
    expect(store.getState().errors).toEqual({});
  });

  it("says Couldn't save when the Companion refuses the config, and keeps what was typed", async () => {
    const store = await detail();
    store.setValue('url', 'ftp://hub.example.com');
    await store.save();
    expect(store.getState().notice).toMatchObject({ kind: 'error', text: "Couldn't save. Check the fields, then retry.", event: 'save-failed', detail: 'Check the Integration fields.' });
    expect(store.getState().values.url).toBe('ftp://hub.example.com');
  });
});

describe('secrets', () => {
  it('never keep the value: not in the state, the Integrations store, the event stream or a URL', async () => {
    const frames: string[] = [];
    const socket = fake.openSocket(`${BASE.replace('http', 'ws')}/api/v1/events`, ['touchdeck.v1', `touchdeck.bearer.${await fake.bridge.session()}`]);
    socket.onmessage = (event) => frames.push(String(event.data));
    const store = await detail();
    await setUp(store);
    await store.refresh();
    await store.load();
    await settle();

    expect(store.getState().secrets.management_key).toEqual({ set: true, hint: 'Set' });
    expect(JSON.stringify(store.getState())).not.toContain(SECRET);
    expect(JSON.stringify(integrations.getState())).not.toContain(SECRET);
    expect(frames.length).toBeGreaterThan(2);
    expect(frames.join('\n')).not.toContain(SECRET);
    expect(requests.some((r) => r.url.includes(SECRET))).toBe(false);
    const carrying = requests.filter((r) => r.body.includes(SECRET));
    expect(carrying.map((r) => [r.method, new URL(r.url).pathname])).toEqual([['PUT', '/api/v1/integrations/cliproxyapi/secrets/management_key']]);
    const view = await companion.integrations.get({ integration: 'cliproxyapi' });
    expect(JSON.stringify(view)).not.toContain(SECRET);
  });

  it('show an empty, write-only field until one is set', async () => {
    const store = await detail();
    const markup = renderToStaticMarkup(<SecretField store={store} field={store.getState().secretFields[0]} integration="CLIProxyAPI" />);
    expect(markup).toContain('data-testid="secret-input"');
    expect(markup).toMatch(/<input[^>]*type="password"[^>]*>/);
    expect(markup).not.toMatch(/<input[^>]*type="password"[^>]*value=/);
    expect(markup).toContain('placeholder="Not set"');
  });

  it('ask for a value before saving', async () => {
    const store = await detail();
    expect(await store.putSecret({ field: 'management_key', value: '   ' })).toBe(false);
    expect(store.getState().errors.management_key).toBe('Enter the management key.');
    expect(requests.some((r) => r.url.includes('/secrets/'))).toBe(false);
  });

  it('are forgotten when a save moves the address to another origin, and the view says so', async () => {
    const store = await detail();
    await setUp(store);
    store.setValue('url', 'https://other-hub.example.com');
    await store.save();
    expect(store.getState().secrets.management_key).toEqual({ set: false, hint: null });
    expect(store.getState().notice).toMatchObject({ kind: 'info', event: 'secret-forgotten', text: 'Saved. The address changed, so the management key was forgotten. Enter it again.' });
  });

  it('stay through a change of path on the same origin', async () => {
    const store = await detail();
    await setUp(store);
    store.setValue('url', 'https://hub.example.com/cliproxy');
    await store.save();
    expect(store.getState().secrets.management_key?.set).toBe(true);
    expect(store.getState().notice?.event).toBe('saved');
  });

  it('can be forgotten', async () => {
    const store = await detail();
    await setUp(store);
    await store.forgetSecret('management_key');
    expect(store.getState().secrets.management_key).toEqual({ set: false, hint: null });
    expect((await companion.integrations.get({ integration: 'cliproxyapi' })).secrets.management_key?.set).toBe(false);
  });
});

describe('Refresh', () => {
  it('asks the hub at most once per floor and says so politely', async () => {
    const store = await detail();
    await setUp(store);
    await store.refresh();
    expect(store.getState().notice?.event).toBe('refreshed');
    expect(fake.integrations.refreshes).toBe(1);

    clock += REFRESH_FLOOR_MS - 1;
    await store.refresh();
    expect(store.getState().notice).toMatchObject({ kind: 'info', event: 'refresh-floor', text: 'Checks are at least 15 s apart, because each one queries your AI services.' });
    expect(fake.integrations.refreshes).toBe(1);

    clock += 1;
    await store.refresh();
    expect(fake.integrations.refreshes).toBe(2);
  });

  it('reads the accounts again after a check', async () => {
    const store = await detail();
    expect(store.getState().usage).toBeNull();
    await setUp(store);
    await store.refresh();
    expect(store.getState().usage?.accounts.map((a) => a.key)).toEqual(['codex:Work', 'claude:Personal']);
    expect(store.getState().usage?.accounts[0].five_hour).toEqual({ pct: 42, reset_in: '2h 10m' });
  });
});
