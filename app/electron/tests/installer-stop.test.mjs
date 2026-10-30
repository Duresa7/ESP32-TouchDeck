import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { copyFile, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { fakeCompanion } from './fake-companion.mjs';

const script = fileURLToPath(new URL('../installer/stop-companion.ps1', import.meta.url));
const cases = [
  ['confirmed lock', { status: 200, body: '{"lock":"test-lock"}' }, 0],
  ['busy', { status: 409, body: '{"error":{"code":"system.busy"}}' }, 3],
  ['unauthorized', { status: 401, body: '{}' }, 4],
  ['server error', { status: 500, body: '{}' }, 4],
  ['malformed JSON', { status: 200, body: '{' }, 4],
  ['missing lock', { status: 200, body: '{}' }, 4],
  ['connection lost after proof', { disconnect: true }, 4],
  ['no listener', null, 0],
  ['wrong proof', null, 0],
];
for (const [name, lockResponse, expected] of cases) {
  test(`installer stop: ${name}`, { skip: process.platform !== 'win32', timeout: 40000 }, async t => {
    const dir = await mkdtemp(join(tmpdir(), 'touchdeck-installer-'));
    const f = await fakeCompanion({ lockResponse, wrongProof: name === 'wrong proof' });
    let target;
    let helper;
    t.after(async () => {
      for (const child of [helper, target]) {
        if (child && child.exitCode === null && child.signalCode === null) { child.kill(); await once(child, 'exit'); }
      }
      if (name !== 'no listener') await f.close();
      await rm(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
    });
    const runtime = join(dir, 'io.github.duresa7.touchdeck', 'runtime');
    await mkdir(runtime, { recursive: true });
    await writeFile(join(runtime, 'companion.json'), JSON.stringify(f.runtime));
    if (name === 'no listener') await f.close();
    await copyFile(join(process.env.WINDIR, 'System32', 'ping.exe'), join(dir, 'td-companion.exe'));
    target = spawn(join(dir, 'td-companion.exe'), ['-t', '127.0.0.1'], { stdio: 'ignore' });
    await once(target, 'spawn');
    helper = spawn(join(process.env.WINDIR, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe'),
      ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', script, '-InstallDir', dir],
      { env: { ...process.env, LOCALAPPDATA: dir }, stdio: ['ignore', 'pipe', 'pipe'] });
    let output = '';
    helper.stdout.on('data', chunk => { output += chunk; });
    helper.stderr.on('data', chunk => { output += chunk; });
    const [code] = await once(helper, 'exit');
    assert.equal(code, expected, output);
    assert.equal(target.exitCode === null, expected !== 0, output);
    assert.equal(f.calls.some(call => call.path === '/api/v1/system/shutdown'), name === 'confirmed lock');
    if (name === 'wrong proof') assert.ok(f.calls.every(call => !call.authorization));
  });
}
