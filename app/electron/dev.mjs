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
let child;
const build = spawnSync('cargo', ['build', '-p', 'td-companion', '--features', 'test-hooks'], { cwd: '..', stdio: 'inherit' });
if (build.status !== 0) process.exit(build.status ?? 1);
const launched = Date.now();
// Spawn the binary itself rather than `cargo run`, so killing it on exit reaches the Companion.
const companion = spawn(env.TOUCHDECK_COMPANION_BIN, ['--mock-devices', '1'], { env, stdio: 'inherit', windowsHide: true });
companion.on('exit', code => { if (!child) process.exit(code ?? 1); });

// Electron starts the Companion itself when none is running, so wait for this one's runtime file first.
const runtimeFile = join(env.TOUCHDECK_APP_DATA_DIR, 'runtime', 'companion.json');
for (;;) {
  try {
    if (JSON.parse(await readFile(runtimeFile, 'utf8')).started_at_ms >= launched) break;
  } catch { /* Not written yet. */ }
  await delay(200);
}
child = spawn(electron, ['.'], { env, stdio: 'inherit', windowsHide: true });
child.on('exit', code => { companion.kill(); process.exitCode = code ?? 1; });
