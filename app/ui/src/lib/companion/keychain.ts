// Where the Companion keeps secrets such as API keys, named for the platform it runs on. The
// Companion uses the OS credential store and never falls back to a file.
import { useContext, useEffect, useState } from 'react';
import type { Platform } from './api.ts';
import type { Companion } from './client.ts';
import { CompanionContext } from './connection.ts';

export function keychainName(platform: Platform | null): string {
  switch (platform) {
    case 'windows':
      return 'Windows Credential Manager';
    case 'linux':
      return 'your system keychain (GNOME Keyring or KWallet)';
    case 'macos':
      return 'the macOS Keychain';
    default:
      return 'your system keychain';
  }
}

const platforms = new WeakMap<object, Promise<Platform | null>>();

function platformOf(companion: Pick<Companion, 'system'>): Promise<Platform | null> {
  let platform = platforms.get(companion);
  if (!platform) {
    platform = companion.system.get().then(
      (info) => info.platform,
      () => {
        // Ask again next time.
        platforms.delete(companion);
        return null;
      },
    );
    platforms.set(companion, platform);
  }
  return platform;
}

/** The credential store's name, asked of the Companion once per connection. Without one, a generic name. */
export function useKeychainName(): string {
  const companion = useContext(CompanionContext);
  const [platform, setPlatform] = useState<Platform | null>(null);
  useEffect(() => {
    if (!companion) return;
    let live = true;
    void platformOf(companion).then((p) => live && setPlatform(p));
    return () => {
      live = false;
    };
  }, [companion]);
  return keychainName(platform);
}
