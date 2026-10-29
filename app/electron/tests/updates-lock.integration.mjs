import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { generateKeyPairSync, randomBytes, sign } from 'node:crypto';
import { createServer } from 'node:http';
import { access, mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { Updater, INSTALLER_ARGS } from '../dist/updates/index.js';

async function until(predicate) {
  for (let n = 0; n < 200; n++) {
    if (await predicate()) return;
    await delay(50);
  }
  assert.fail('Timed out waiting for Companion');
}

async function fixture(t) {
  const binary = process.env.TOUCHDECK_COMPANION_BIN || fileURLToPath(new URL('../../target/debug/td-companion' + (process.platform === 'win32' ? '.exe' : ''), import.meta.url));
  await access(binary);
  const dir = await mkdtemp(join(tmpdir(), 'touchdeck-lock-'));
  const child = spawn(binary, ['--mock-devices', '1'], {
    env: { ...process.env, TOUCHDECK_APP_DATA_DIR: dir }, windowsHide: true, stdio: 'ignore',
  });
  const exited = once(child, 'exit');
  t.after(async () => {
    if (child.exitCode === null && child.signalCode === null) child.kill();
    await exited;
    await rm(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  });
  const runtimeDir = join(dir, 'runtime');
  let runtime;
  await until(async () => {
    try { runtime = JSON.parse(await readFile(join(runtimeDir, 'companion.json'), 'utf8')); return true; }
    catch { return false; }
  });
  const origin = `http://127.0.0.1:${runtime.port}`;
  async function call(token, method, path, body) {
    const res = await fetch(`${origin}/api/v1${path}`, { method,
      headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(5000),
    });
    return { status: res.status, body: res.status === 204 ? null : await res.json() };
  }
  async function session() {
    const res = await call(runtime.token, 'POST', '/auth/sessions', { kind: 'window' });
    assert.equal(res.status, 200);
    return res.body;
  }
  const owner = await session(), other = await session();
  const frames = [];
  const socket = new WebSocket(`${origin.replace('http:', 'ws:')}/api/v1/events`, ['touchdeck.v1', `touchdeck.bearer.${owner.token}`]);
  socket.addEventListener('message', event => frames.push(JSON.parse(event.data)));
  t.after(() => socket.close());
  await until(() => frames.some(frame => frame.type === 'sync'));
  await until(async () => (await call(owner.token, 'GET', '/devices')).body.some(d => d.id === 'mock0001' && d.link.state === 'connected'));
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const id = randomBytes(8), data = randomBytes(4096);
  const key = Buffer.concat([Buffer.from('Ed'), id, publicKey.export({ type: 'spki', format: 'der' }).subarray(-32)]);
  const sig = sign(null, data, privateKey), comment = 'timestamp:1\tfile:setup.exe\tversion:99.0.0';
  const packet = Buffer.concat([Buffer.from('Ed'), id, sig]);
  const signature = Buffer.from(`untrusted comment: test\n${packet.toString('base64')}\ntrusted comment: ${comment}\n${sign(null, Buffer.concat([sig, Buffer.from(comment)]), privateKey).toString('base64')}\n`).toString('base64');
  let feedOrigin;
  const feed = createServer((req, res) => res.end(req.url === '/latest.json'
    ? JSON.stringify({ version: '99.0.0', platforms: { 'windows-x86_64': { url: `${feedOrigin}/setup.exe`, signature } } }) : data));
  await new Promise(resolve => feed.listen(0, '127.0.0.1', resolve));
  feedOrigin = `http://127.0.0.1:${feed.address().port}`;
  t.after(async () => { feed.closeAllConnections(); await new Promise(resolve => feed.close(resolve)); });
  const state = { calls: [], launches: [], afterLock: async () => {}, quit: false };
  const updater = new Updater({ publicKey: Buffer.from(`untrusted comment: test\n${key.toString('base64')}\n`).toString('base64'),
    current: '0.2.0', runtimeDir, feed: `${feedOrigin}/latest.json`, allowLoopback: true,
    system: () => async (method, path) => {
      const result = await call(owner.token, method, path);
      state.calls.push({ method, path, ...result });
      if (method === 'POST' && path === '/system/lock' && result.status === 200) await state.afterLock();
      return result;
    },
    launch: async (path, args) => {
      state.launches.push({ bytes: await readFile(path), args,
        stopped: child.exitCode !== null || frames.some(frame => frame.type === 'companion.stopping') });
    }, quit: () => { state.quit = true; },
  });
  assert.equal((await updater.check()).state, 'available');
  assert.deepEqual(await updater.download(() => {}), { state: 'ready' });
  return { updater, state, call, owner, other, session, data,
    staging: async () => (await readdir(runtimeDir)).filter(name => name.startsWith('update-')) };
}

async function launched(f) {
  void f.updater.install();
  await until(() => f.state.quit);
  assert.equal(f.state.launches.length, 1);
  assert.deepEqual(f.state.launches[0].bytes, f.data);
  assert.deepEqual(f.state.launches[0].args, INSTALLER_ARGS);
}

function noHandoff(f) {
  assert.equal(f.state.launches.length, 0);
  assert.equal(f.state.calls.some(call => call.path === '/system/shutdown'), false);
}

const layout = { schema: 1, name: 'Lock test', orientation: 'landscape', pages: [
  { id: 'main', title: 'Main', kind: 'grid', rows: 2, cols: 2, tiles: [] },
] };
const layoutPath = '/devices/mock0001/layouts/landscape';

test('real lock: busy Job retains installer and retries after Job ends', async t => {
  const f = await fixture(t);
  assert.equal((await f.call(f.owner.token, 'POST', '/test/devices/mock0001/mock', { action: 'stop' })).status, 204);
  const job = await f.call(f.owner.token, 'PUT', layoutPath, layout);
  assert.equal(job.status, 202);
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'busy', device: null });
  noHandoff(f);
  assert.equal((await f.staging()).length, 1);
  await until(async () => ['failed', 'cancelled', 'succeeded'].includes((await f.call(f.owner.token, 'GET', `/jobs/${job.body.id}`)).body.state));
  await launched(f);
});

test('real lock: another window owns renew and release; busy retains installer', async t => {
  const f = await fixture(t);
  const acquired = await f.call(f.other.token, 'POST', '/system/lock');
  assert.equal(acquired.status, 200);
  const path = `/system/lock/${acquired.body.lock}`;
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'busy', device: null });
  noHandoff(f);
  assert.equal((await f.staging()).length, 1);
  for (const method of ['PUT', 'DELETE']) {
    const denied = await f.call(f.owner.token, method, path);
    assert.equal(denied.status, 403);
    assert.equal(denied.body.error.code, 'auth.forbidden');
  }
  assert.equal((await f.call(f.other.token, 'PUT', path)).status, 204);
  assert.equal((await f.call(f.other.token, 'DELETE', path)).status, 204);
  await launched(f);
});

for (const loss of ['lease expiry', 'revoke']) test(`real lock: ${loss} before renew removes staging without handoff`, async t => {
  const f = await fixture(t);
  f.state.afterLock = async () => {
    const res = loss === 'lease expiry'
      ? await f.call(f.owner.token, 'POST', '/test/clock', { advance_ms: 600001 })
      : await f.call(f.other.token, 'DELETE', `/auth/sessions/${f.owner.id}`);
    assert.equal(res.status, 204);
  };
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'installer' });
  noHandoff(f);
  const renew = f.state.calls.find(call => call.method === 'PUT');
  assert.equal(renew.status, loss === 'lease expiry' ? 403 : 401);
  assert.equal(renew.body.error.code, loss === 'lease expiry' ? 'auth.forbidden' : 'auth.revoked');
  assert.deepEqual(await f.staging(), []);
  const fresh = await f.session();
  assert.equal((await f.call(fresh.token, 'POST', '/system/lock')).status, 200);
});

test('real lock: shutdown draining precedes launch and new Jobs are refused', async t => {
  const f = await fixture(t);
  f.state.afterLock = async () => {
    const denied = await f.call(f.other.token, 'PUT', layoutPath, layout);
    assert.equal(denied.status, 503);
    assert.equal(denied.body.error.code, 'companion.installing');
  };
  await launched(f);
  assert.deepEqual(f.state.calls.map(({ method, path, status }) => [method, path.split('/')[2], status]),
    [['POST', 'lock', 200], ['PUT', 'lock', 204], ['POST', 'shutdown', 204]]);
  assert.equal(f.state.launches[0].stopped, true, 'Companion exit or companion.stopping must precede launch');
});
