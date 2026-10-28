import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { createClient } from './client.ts';
import { ApiError, isApiError, toApiError } from './errors.ts';
import { createFakeCompanion, type FakeCompanion } from './fake.ts';
import { sampleDevice } from './samples.ts';
import { BROWSER_SESSION_KEY, browserSession, windowSession } from './session.ts';

const BASE = 'http://127.0.0.1:51234';

function memoryStorage(initial: Record<string, string> = {}) {
  const items = new Map(Object.entries(initial));
  return {
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
    removeItem: (key: string) => void items.delete(key),
  };
}

let fake: FakeCompanion;

beforeEach(() => {
  fake = createFakeCompanion({ devices: [sampleDevice('mock0001', 'Desk')], system: { version: '1.2.3', build: 'abc1234' } });
  vi.stubGlobal('fetch', fake.fetch);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('error parsing', () => {
  test('an error body becomes an ApiError with its code, message and details', () => {
    const error = toApiError({ code: 'layout.invalid', message: 'The Layout has 1 problem.', details: { issues: [] } });
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('layout.invalid');
    expect(error.message).toBe('The Layout has 1 problem.');
    expect(error.details).toEqual({ issues: [] });
  });

  test('a whole error response is unwrapped to its error body', () => {
    expect(toApiError({ error: { code: 'device.busy', message: 'A Job holds the Device.', details: { job: 'job_1' } } }).code).toBe('device.busy');
  });

  test('a failed fetch is unreachable and an answer without the error shape is a bad response', () => {
    expect(toApiError(new TypeError('Failed to fetch')).code).toBe('client.unreachable');
    expect(toApiError(new SyntaxError('Unexpected token <')).code).toBe('client.bad_response');
    expect(toApiError(undefined).code).toBe('client.bad_response');
  });

  test('isApiError narrows on the code', () => {
    const error: unknown = new ApiError('device.not_found', 'No Device with that id.');
    expect(isApiError(error)).toBe(true);
    expect(isApiError(error, 'device.not_found')).toBe(true);
    expect(isApiError(error, 'auth.revoked')).toBe(false);
    expect(isApiError(new Error('plain'))).toBe(false);
  });

  test('a Companion error reaches the caller as an ApiError', async () => {
    const companion = createClient(BASE, windowSession(fake.bridge));
    const error = await companion.devices.get({ device: 'nope' }).catch((e: unknown) => e);
    expect(isApiError(error, 'device.not_found')).toBe(true);
  });

  test('an answer that is not JSON reaches the caller as a bad response', async () => {
    vi.stubGlobal('fetch', () => Promise.resolve(new Response('<html>Bad gateway</html>', { status: 502 })));
    const companion = createClient(BASE, windowSession(fake.bridge));
    const error = await companion.system.get().catch((e: unknown) => e);
    expect(isApiError(error, 'client.bad_response')).toBe(true);
  });
});

describe('calls', () => {
  test('system.get and devices.list answer with the typed objects', async () => {
    const companion = createClient(BASE, windowSession(fake.bridge));
    expect(await companion.system.get()).toMatchObject({ version: '1.2.3', build: 'abc1234', api: 'v1' });
    const devices = await companion.devices.list();
    expect(devices.map((d) => d.name)).toEqual(['Desk']);
  });
});

describe('sessions', () => {
  test('the window asks Electron for one session and reuses it', async () => {
    const companion = createClient(BASE, windowSession(fake.bridge));
    await companion.system.get();
    await companion.devices.list();
    expect(fake.minted).toBe(1);
  });

  test('a 401 asks Electron for a new session and retries once', async () => {
    const companion = createClient(BASE, windowSession(fake.bridge));
    await companion.system.get();
    fake.revokeSessions();
    expect(await companion.system.get()).toMatchObject({ version: '1.2.3' });
    expect(fake.minted).toBe(2);
  });

  test('calls that fail together share one new session', async () => {
    const companion = createClient(BASE, windowSession(fake.bridge));
    await companion.system.get();
    fake.revokeSessions();
    await Promise.all([companion.system.get(), companion.devices.list(), companion.devices.get({ device: 'mock0001' })]);
    expect(fake.minted).toBe(2);
  });

  test('a browser sends its stored token', async () => {
    const token = await fake.bridge.session();
    const companion = createClient(BASE, browserSession(memoryStorage({ [BROWSER_SESSION_KEY]: token! })));
    expect(await companion.devices.list()).toHaveLength(1);
  });

  test('a browser whose session ended clears its token and gets the auth error, so it can pair again', async () => {
    const token = await fake.bridge.session();
    const storage = memoryStorage({ [BROWSER_SESSION_KEY]: token! });
    const companion = createClient(BASE, browserSession(storage));
    fake.revokeSessions();
    const before = fake.requests;
    const error = await companion.devices.list().catch((e: unknown) => e);
    expect(isApiError(error, 'auth.revoked')).toBe(true);
    expect(storage.getItem(BROWSER_SESSION_KEY)).toBeNull();
    expect(fake.requests - before).toBe(1);
  });

  test('a browser that never paired gets auth.missing', async () => {
    const companion = createClient(BASE, browserSession(memoryStorage()));
    const error = await companion.system.get().catch((e: unknown) => e);
    expect(isApiError(error, 'auth.missing')).toBe(true);
  });

  test('a session that cannot be fetched is unreachable, and nothing is sent', async () => {
    const bridge = { ...fake.bridge, session: () => Promise.reject(new TypeError('Failed to fetch')) };
    const companion = createClient(BASE, windowSession(bridge));
    const error = await companion.system.get().catch((e: unknown) => e);
    expect(isApiError(error, 'client.unreachable')).toBe(true);
    expect(fake.requests).toBe(0);
  });
});
