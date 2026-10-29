import { spawn, spawnSync } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import electron from 'electron';

// Dev runs the Companion with one mock Device in a scratch data dir, so it never meets an installed Companion or COM ports.
const env = { ...process.env, TOUCHDECK_APP_DATA_DIR: process.env.TOUCHDECK_APP_DATA_DIR || join(tmpdir(), 'touchdeck-dev') };
delete env.ELECTRON_RUN_AS_NODE;
env.TOUCHDECK_COMPANION_BIN = resolve('../target/debug/td-companion' + (process.platform === 'win32' ? '.exe' : ''));
const children = [];
const stop = code => { for (const c of children) c.kill(); process.exit(code ?? 1); };

const build = spawnSync('cargo', ['build', '-p', 'td-companion', '--features', 'test-hooks'], { cwd: '..', stdio: 'inherit' });
if (build.status !== 0) process.exit(build.status ?? 1);
const launched = Date.now();
// Spawn the binaries themselves rather than through cargo or pnpm, so killing them on exit reaches them.
const companion = spawn(env.TOUCHDECK_COMPANION_BIN, ['--mock-devices', '1'], { env, stdio: 'inherit', windowsHide: true });
children.push(companion);
companion.on('exit', stop);

// Electron starts the Companion itself when none is running, so wait for this one's runtime file first.
const runtimeFile = join(env.TOUCHDECK_APP_DATA_DIR, 'runtime', 'companion.json');
for (;;) {
  try {
    if (JSON.parse(await readFile(runtimeFile, 'utf8')).started_at_ms >= launched) break;
  } catch { /* Not written yet. */ }
  await delay(200);
}

// The UI dev server reads the runtime file at start and proxies /api to the Companion.
const ui = 'http://127.0.0.1:5174/';
const vite = spawn(process.execPath, [resolve('../ui/node_modules/vite/bin/vite.js')],
  { cwd: '../ui', env, stdio: 'inherit', windowsHide: true });
children.push(vite);
vite.on('exit', stop);
for (;;) {
  try { if ((await fetch(ui)).ok) break; } catch { /* Not listening yet. */ }
  await delay(200);
}

const shell = spawn(electron, ['.'], { env: { ...env, TOUCHDECK_UI_DEV_URL: ui }, stdio: 'inherit', windowsHide: true });
shell.on('exit', stop);
