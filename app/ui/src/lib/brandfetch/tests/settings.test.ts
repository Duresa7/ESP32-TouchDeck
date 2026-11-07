import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createClient } from '../../companion/client.ts';
import { ApiError } from '../../companion/errors.ts';
import { createFakeCompanion, type FakeCompanion } from '../../companion/fake.ts';
import { windowSession } from '../../companion/session.ts';
import { brandfetchSource, pickProblem, RECENT_NAME } from '../pick.ts';
import { clientIdOf, createBrandfetchStore, logoCount } from '../settings.ts';

const BASE = 'http://127.0.0.1:51234';
let fake: FakeCompanion;

beforeEach(() => {
  fake = createFakeCompanion();
  vi.stubGlobal('fetch', fake.fetch);
});
afterEach(() => vi.unstubAllGlobals());

const store = () => createBrandfetchStore(createClient(BASE, windowSession(fake.bridge)));

describe('Brandfetch settings', () => {
  it('turn search on with a client ID and keep the key write-only', async () => {
    const settings = store();
    await settings.getState().load();
    expect(clientIdOf(settings.getState().view)).toBeNull();
    expect(await settings.getState().saveClientId(' test-client-id ')).toBe(true);
    expect(clientIdOf(settings.getState().view)).toBe('test-client-id');
    expect(await settings.getState().saveKey('test-api-key')).toBe(true);
    const view = settings.getState().view;
    expect(view.kind === 'ready' && view.status.key_set).toBe(true);
    expect(JSON.stringify(view)).not.toContain('test-api-key');
    expect(JSON.stringify(fake.brandfetch.status())).not.toContain('test-api-key');
    expect(await settings.getState().forgetClientId()).toBe(true);
    expect(clientIdOf(settings.getState().view)).toBeNull();
  });

  it('test the key only when asked, with one call', async () => {
    const settings = store();
    await settings.getState().load();
    expect(await settings.getState().testKey()).toBe(false);
    const before = settings.getState().view;
    expect(before.kind === 'ready' && before.problem).toBe('Save an API key first.');
    expect(await settings.getState().saveKey('test-api-key')).toBe(true);
    expect(fake.brandfetch.keyTests).toBe(0);
    expect(await settings.getState().testKey()).toBe(true);
    expect(fake.brandfetch.keyTests).toBe(1);
    const after = settings.getState().view;
    expect(after.kind === 'ready' && after.notice).toBe('Brandfetch accepted the key.');
  });

  it('say how to start the system keychain when there is none', async () => {
    fake = createFakeCompanion({ brandfetch: { keychain: false } });
    vi.stubGlobal('fetch', fake.fetch);
    const settings = store();
    await settings.getState().load();
    expect(await settings.getState().saveKey('test-api-key')).toBe(false);
    const view = settings.getState().view;
    expect(view.kind === 'ready' && view.problem).toBe('No system keychain is available. Start GNOME Keyring or KWallet and try again.');
    expect(view.kind === 'ready' && view.status.key_set).toBe(false);
  });

  it('explain a rejected client ID', async () => {
    const settings = store();
    await settings.getState().load();
    expect(await settings.getState().saveClientId('has space')).toBe(false);
    const view = settings.getState().view;
    expect(view.kind === 'ready' && view.problem).toMatch(/client ID/);
  });

  it('say the Companion is unreachable', async () => {
    vi.stubGlobal('fetch', () => Promise.reject(new TypeError('Failed to fetch')));
    const settings = store();
    await settings.getState().load();
    expect(settings.getState().view).toEqual({ kind: 'unreadable', problem: expect.stringMatching(/Can't reach the Companion/) });
  });

  it('count logos in plain words', () => {
    expect(logoCount({ logos: 0, need_refresh: 0 })).toBeNull();
    expect(logoCount({ logos: 1, need_refresh: 0 })).toBe('1 logo, all up to date');
    expect(logoCount({ logos: 3, need_refresh: 1 })).toBe('3 logos; 1 needs a refresh');
    expect(logoCount({ logos: 3, need_refresh: 2 })).toBe('3 logos; 2 need a refresh');
  });
});

describe('Brandfetch picks', () => {
  it('ask the Companion for the brand by domain and remember no brand data', () => {
    expect(brandfetchSource({ domain: 'music.example', brandId: 'id_fixture_music' })).toEqual({
      kind: 'brandfetch',
      brand: 'music.example',
      logo_type: 'icon',
      theme: 'auto',
      padding: 0,
      background: null,
      tint: null,
    });
    expect(brandfetchSource({ domain: '', brandId: 'id_fixture_music' }).brand).toBe('id_fixture_music');
    expect(RECENT_NAME).toBe('Brandfetch logo');
  });

  it.each([
    ['brandfetch.no_key', /API key in Settings/],
    ['brandfetch.key_refused', /refused the API key/],
    ['brandfetch.quota', /used up/],
    ['brandfetch.not_found', /no details for Example/],
    ['brandfetch.no_logo', /no icon for Example/],
    ['brandfetch.unavailable', /internet connection/],
    ['brandfetch.no_keychain', /^x$/],
  ] as const)('explain %s', (code, message) => {
    expect(pickProblem(new ApiError(code, 'x'), 'Example')).toMatch(message);
  });
});
