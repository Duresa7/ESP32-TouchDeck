import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';

/**
 * Fetches the Electron binary before any spec runs.
 *
 * Electron 45 has no install script; `require('electron')` downloads the binary on first use with a
 * blocking `spawnSync`. Without this, the first `electron.launch` in a fresh install does that download
 * inside a spec's 30 s timeout and stalls the worker's event loop while it runs.
 */
export default function globalSetup() {
  const binary: string = createRequire(__filename)('electron');
  if (!existsSync(binary)) throw new Error(`Electron binary missing at ${binary}`);
}
