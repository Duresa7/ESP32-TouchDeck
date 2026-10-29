import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { createClient } from '../../companion/client.ts';
import { createFakeCompanion, type FakeCompanion } from '../../companion/fake.ts';
import { windowSession } from '../../companion/session.ts';
import { UNREACHABLE } from './problems.ts';
import { createStartupStore } from './startup.ts';

const BASE = 'http://127.0.0.1:51234';

let fake: FakeCompanion;

beforeEach(() => {
  fake = createFakeCompanion();
  vi.stubGlobal('fetch', fake.fetch);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const store = () => createStartupStore(createClient(BASE, windowSession(fake.bridge)));

describe('Start at sign-in', () => {
  test('reads the setting when it opens', async () => {
    fake = createFakeCompanion({ startAtSignIn: true });
    vi.stubGlobal('fetch', fake.fetch);
    const startup = store();
    expect(startup.getState().view).toEqual({ kind: 'loading' });
    await startup.getState().load();
    expect(startup.getState().view).toEqual({ kind: 'ready', on: true, problem: null });
  });

  test('shows the new value before the Companion answers, then keeps it', async () => {
    const startup = store();
    await startup.getState().load();
    const write = startup.getState().set(true);
    expect(startup.getState().view).toEqual({ kind: 'ready', on: true, problem: null });
    await write;
    expect(startup.getState().view).toEqual({ kind: 'ready', on: true, problem: null });
    expect(fake.startup.atSignIn).toBe(true);
  });

  test('goes back to the saved value and says why when the write fails', async () => {
    const startup = store();
    await startup.getState().load();
    fake.startup.fail(true);
    const write = startup.getState().set(true);
    expect(startup.getState().view).toMatchObject({ on: true });
    await write;
    expect(startup.getState().view).toEqual({
      kind: 'ready',
      on: false,
      problem: "Couldn't turn on Start at sign-in. Retry, or restart Open TouchDeck.",
    });
    expect(fake.startup.atSignIn).toBe(false);

    fake.startup.fail(false);
    await startup.getState().set(true);
    expect(startup.getState().view).toEqual({ kind: 'ready', on: true, problem: null });
  });

  test('a quick second click wins, and a failure after a saved write rolls back to that write', async () => {
    const startup = store();
    await startup.getState().load();
    const first = startup.getState().set(true);
    const second = startup.getState().set(false);
    expect(startup.getState().view).toMatchObject({ on: false });
    await Promise.all([first, second]);
    expect(startup.getState().view).toEqual({ kind: 'ready', on: false, problem: null });
    expect(fake.startup.atSignIn).toBe(false);

    await startup.getState().set(true);
    fake.startup.fail(true);
    await startup.getState().set(false);
    expect(startup.getState().view).toMatchObject({ on: true, problem: "Couldn't turn off Start at sign-in. Retry, or restart Open TouchDeck." });
  });

  test('an unreachable Companion gets the copy sentence, and the switch stays unusable until it reads', async () => {
    vi.stubGlobal('fetch', () => Promise.reject(new TypeError('Failed to fetch')));
    const startup = store();
    await startup.getState().load();
    expect(startup.getState().view).toEqual({ kind: 'unreadable', problem: UNREACHABLE });
    await startup.getState().set(true);
    expect(startup.getState().view.kind).toBe('unreadable');
  });

  test('a Companion without the route reads as unreadable, not as off', async () => {
    vi.stubGlobal('fetch', (input: RequestInfo | URL, init?: RequestInit) => {
      const url = new URL(input instanceof Request ? input.url : String(input));
      return url.pathname === '/api/v1/system/startup'
        ? Promise.resolve(new Response(JSON.stringify({ error: { code: 'route.not_found', message: 'No route with that path.', details: null } }), { status: 404 }))
        : fake.fetch(input, init);
    });
    const startup = store();
    await startup.getState().load();
    expect(startup.getState().view).toEqual({ kind: 'unreadable', problem: "Couldn't read Start at sign-in. Restart Open TouchDeck, then retry." });
  });
});
