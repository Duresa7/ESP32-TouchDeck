import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { createClient } from '../../companion/client.ts';
import { createFakeCompanion, type FakeCompanion } from '../../companion/fake.ts';
import { windowSession } from '../../companion/session.ts';
import { createBrowsersStore, type BrowsersStore } from './browsers.ts';
import { UNREACHABLE } from './problems.ts';

const BASE = 'http://127.0.0.1:51234';
const EDGE = { label: 'Edge on Windows', created_ms: 1000, last_used_ms: 5000 };
const FIREFOX = { label: 'Firefox on Windows', created_ms: 2000, last_used_ms: 3000 };

let fake: FakeCompanion;
let browsers: BrowsersStore;

const labels = () => {
  const { view } = browsers.getState();
  return view.kind === 'ready' ? view.browsers.map((b) => b.label) : view.kind;
};
const live = () => fake.sessions.list().filter((s) => s.kind === 'browser').map((s) => s.label);
const browser = (label: string) => {
  const { view } = browsers.getState();
  const found = view.kind === 'ready' ? view.browsers.find((b) => b.label === label) : undefined;
  if (!found) throw new Error(`no browser ${label}`);
  return found;
};

function failDeletes() {
  vi.stubGlobal('fetch', (input: RequestInfo | URL, init?: RequestInit) =>
    init?.method === 'DELETE' ? Promise.reject(new TypeError('Failed to fetch')) : fake.fetch(input, init),
  );
}

beforeEach(async () => {
  fake = createFakeCompanion({ browsers: [FIREFOX, EDGE] });
  vi.stubGlobal('fetch', fake.fetch);
  browsers = createBrowsersStore(createClient(BASE, windowSession(fake.bridge)));
  await browsers.getState().load();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('Paired browsers', () => {
  test('lists only browser sessions, oldest first, not the window', async () => {
    expect(labels()).toEqual(['Edge on Windows', 'Firefox on Windows']);
    expect(fake.sessions.list().some((s) => s.kind === 'window')).toBe(true);
  });

  test('Forget takes the row away at once and the Companion forgets the browser', async () => {
    const forget = browsers.getState().forget(browser('Edge on Windows'));
    expect(labels()).toEqual(['Firefox on Windows']);
    await forget;
    expect(live()).toEqual(['Firefox on Windows']);
    await browsers.getState().load();
    expect(labels()).toEqual(['Firefox on Windows']);
    expect(browsers.getState().problem).toBeNull();
  });

  test('a failed Forget puts the row back in its place and says why', async () => {
    failDeletes();
    await browsers.getState().forget(browser('Edge on Windows'));
    expect(labels()).toEqual(['Edge on Windows', 'Firefox on Windows']);
    expect(browsers.getState().problem).toBe(UNREACHABLE);
    expect(live()).toHaveLength(2);
  });

  test('Forget all sends one bulk request then reloads the list', async () => {
    const fetch = vi.fn(fake.fetch);
    vi.stubGlobal('fetch', fetch);
    await browsers.getState().forgetAll();
    expect(labels()).toEqual([]);
    expect(live()).toEqual([]);
    expect(fetch.mock.calls.map(([input, init]) => [String(input), init?.method])).toEqual([
      [`${BASE}/api/v1/auth/sessions?kind=browser`, 'DELETE'],
      [`${BASE}/api/v1/auth/sessions`, 'GET'],
    ]);
  });

  test('a failed Forget all shows what the Companion still has', async () => {
    failDeletes();
    await browsers.getState().forgetAll();
    expect(labels()).toEqual(['Edge on Windows', 'Firefox on Windows']);
    expect(browsers.getState().problem).toBe(UNREACHABLE);
  });

  test('Reset pairing ends every session, and this window gets a new one to list with', async () => {
    const minted = fake.minted;
    await browsers.getState().resetPairing();
    expect(labels()).toEqual([]);
    expect(fake.minted).toBe(minted + 1);
    expect(fake.sessions.list().map((s) => s.kind)).toEqual(['window']);
    expect(browsers.getState().problem).toBeNull();
  });
});
