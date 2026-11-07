import { afterEach, describe, expect, it, vi } from 'vitest';
import { createClient } from '../client.ts';
import { createFakeCompanion } from '../fake.ts';
import { keychainName } from '../keychain.ts';
import { windowSession } from '../session.ts';

afterEach(() => vi.unstubAllGlobals());

describe('the keychain name', () => {
  it.each([
    ['windows', 'Windows Credential Manager'],
    ['linux', 'your system keychain (GNOME Keyring or KWallet)'],
    ['macos', 'the macOS Keychain'],
    ['other', 'your system keychain'],
    [null, 'your system keychain'],
  ] as const)('on %s is %s', (platform, name) => {
    expect(keychainName(platform)).toBe(name);
  });

  it('follows the platform the Companion reports', async () => {
    const fake = createFakeCompanion({ system: { platform: 'linux' } });
    vi.stubGlobal('fetch', fake.fetch);
    const companion = createClient('http://127.0.0.1:51234', windowSession(fake.bridge));
    const info = await companion.system.get();
    expect(keychainName(info.platform)).toBe('your system keychain (GNOME Keyring or KWallet)');
  });
});
