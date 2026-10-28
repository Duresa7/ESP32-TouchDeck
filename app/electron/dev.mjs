import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import electron from 'electron';
import { fakeCompanion } from './tests/fake-companion.mjs';

// Phase A runs without hardware or the legacy UI. Phase B supplies the real Companion.
const real = existsSync('../crates/td-companion/Cargo.toml');
let fake;
let companion;
const env = { ...process.env };
delete env.ELECTRON_RUN_AS_NODE;
if (real) {
  companion = spawn('cargo', ['run', '-p', 'td-companion', '--features', 'test-hooks', '--', '--mock-devices', '1'],
    { cwd: '..', stdio: 'inherit', windowsHide: true });
  env.TOUCHDECK_COMPANION_BIN = resolve('../target/debug/td-companion' + (process.platform === 'win32' ? '.exe' : ''));
} else {
  fake = await fakeCompanion();
  env.TOUCHDECK_APP_DATA_DIR = fake.dir;
}
const child = spawn(electron, ['.'], { env, stdio: 'inherit', windowsHide: true });
child.on('exit', async code => { companion?.kill(); await fake?.close(); process.exitCode = code ?? 1; });
