import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto';
import { createServer } from 'node:http';
import { mkdtemp, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Updater, feedSettings, builtInKey, INSTALLER_ARGS } from '../dist/updates/index.js';
import { compareVersions, verifyInstaller } from '../dist/updates/signature.js';

function signer() {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const id = randomBytes(8);
  const key = Buffer.concat([Buffer.from('Ed'), id, publicKey.export({ type: 'spki', format: 'der' }).subarray(-32)]);
  return {
    key: Buffer.from(`untrusted comment: test\n${key.toString('base64')}\n`).toString('base64'),
    sign(data, { legacy = false, version = '1.0.0', wrongId = false, badComment = false } = {}) {
      const sig = sign(null, legacy ? data : createHash('blake2b512').update(data).digest(), privateKey);
      const comment = `timestamp:1\tfile:setup.exe\tversion:${version}`;
      const global = sign(null, Buffer.concat([sig, Buffer.from(comment)]), privateKey);
      if (badComment) global[0] ^= 1;
      const packet = Buffer.concat([Buffer.from(legacy ? 'Ed' : 'ED'), wrongId ? randomBytes(8) : id, sig]);
      return Buffer.from(`untrusted comment: test\n${packet.toString('base64')}\ntrusted comment: ${comment}\n${global.toString('base64')}\n`).toString('base64');
    },
  };
}
async function fixture(t) {
  const dir = await mkdtemp(join(tmpdir(), 'touchdeck-update-'));
  const signing = signer();
  const data = randomBytes(64 * 1024);
  const state = { data, status: 200, manifest: null, calls: [], busy: false, failRenew: false, launches: [], quits: 0 };
  const server = createServer((req, res) => {
    if (req.url === '/latest.json') { res.writeHead(state.status); res.end(JSON.stringify(state.manifest)); }
    else if (req.url === '/redirect') { res.writeHead(302, { location: 'http://example.invalid/setup.exe' }); res.end(); }
    else { res.end(state.data); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  state.manifest = { version: '1.0.0', platforms: { 'windows-x86_64': { url: `${origin}/setup.exe`, signature: signing.sign(data) } } };
  const options = { publicKey: signing.key, current: '0.2.0', dataDir: dir,
    ...feedSettings(false, { TOUCHDECK_UPDATE_TEST_FEED: `${origin}/latest.json` }),
    system: () => async (method, path) => {
      state.calls.push([method, path]);
      if (method === 'POST' && path === '/system/lock') return state.busy
        ? { status: 409, body: { error: { code: 'system.busy', details: { device: 'Studio' } } } }
        : { status: 200, body: { lock: 'test-lock' } };
      return { status: state.failRenew && method === 'PUT' ? 403 : 204, body: null };
    },
    launch: async (path, args) => { state.launches.push({ data: await readFile(path), args }); }, quit: () => { state.quits++; },
  };
  const updater = new Updater(options);
  t.after(async () => { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); await rm(dir, { recursive: true, force: true }); });
  return { dir, state, options, updater, signing, server };
}

test('strict semver precedence including prereleases and build metadata', () => {
  const ordered = ['1.0.0-alpha', '1.0.0-alpha.1', '1.0.0-alpha.beta', '1.0.0-beta', '1.0.0-beta.2', '1.0.0-beta.11', '1.0.0-rc.1', '1.0.0'];
  for (let i = 1; i < ordered.length; i++) assert.equal(compareVersions(ordered[i], ordered[i - 1]), 1);
  assert.equal(compareVersions('v1.0.0+a', '1.0.0+b'), 0);
  assert.throws(() => compareVersions('1.0.0-01', '1.0.0'));
});

test('disabled source build and packaged build ignores test feed', async t => {
  const f = await fixture(t);
  assert.deepEqual(await new Updater({ ...f.options, publicKey: '' }).check(), { state: 'disabled' });
  assert.equal(await builtInKey(f.dir), '');
  assert.equal(feedSettings(true, { TOUCHDECK_UPDATE_TEST_FEED: 'http://127.0.0.1:1' }).allowLoopback, false);
  assert.throws(() => feedSettings(false, { TOUCHDECK_UPDATE_TEST_FEED: 'http://example.invalid' }));
});

test('available, none, failed and offline from a real local feed', async t => {
  const f = await fixture(t);
  assert.deepEqual(await f.updater.check(), { state: 'available', version: '1.0.0', current: '0.2.0', notes: null, date: null });
  f.state.manifest.version = '0.2.0'; assert.deepEqual(await f.updater.check(), { state: 'none' });
  f.state.manifest.version = 'bad'; assert.deepEqual(await f.updater.check(), { state: 'failed' });
  f.server.closeAllConnections(); await new Promise(resolve => f.server.close(resolve));
  assert.deepEqual(await f.updater.check(), { state: 'offline' });
});

for (const legacy of [false, true]) test(`verified download and progress, legacy=${legacy}`, async t => {
  const f = await fixture(t);
  f.state.manifest.platforms['windows-x86_64'].signature = f.signing.sign(f.state.data, { legacy });
  await f.updater.check(); const progress = [];
  assert.deepEqual(await f.updater.download(p => progress.push(p)), { state: 'ready' });
  assert.equal(progress.at(-1).downloaded, f.state.data.length);
  assert.equal(progress.at(-1).total, f.state.data.length);
});

for (const failure of ['signature', 'wrongId', 'badComment', 'truncated', 'version', 'oversized']) test(`refuses and deletes ${failure}`, async t => {
  const f = await fixture(t); const entry = f.state.manifest.platforms['windows-x86_64'];
  if (failure === 'signature') f.state.data[0] ^= 1;
  if (failure === 'wrongId' || failure === 'badComment') entry.signature = f.signing.sign(f.state.data, { [failure]: true });
  if (failure === 'truncated') f.state.data = f.state.data.subarray(0, 20);
  if (failure === 'version') entry.signature = f.signing.sign(f.state.data, { version: '0.1.0' });
  if (failure === 'oversized') entry.size = 20;
  await f.updater.check();
  assert.deepEqual(await f.updater.download(() => {}), { state: 'failed', reason: failure === 'oversized' ? 'download' : 'signature' });
  assert.deepEqual(await readdir(f.dir), []);
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'installer' });
  assert.equal(f.state.launches.length, 0);
});

test('rejects insecure redirect and serializes downloads', async t => {
  const f = await fixture(t);
  f.state.manifest.platforms['windows-x86_64'].url = f.options.feed.replace('latest.json', 'redirect');
  await f.updater.check(); assert.deepEqual(await f.updater.download(() => {}), { state: 'failed', reason: 'download' });
  f.state.manifest.platforms['windows-x86_64'].url = f.options.feed.replace('latest.json', 'setup.exe');
  await f.updater.check(); const first = f.updater.download(() => {});
  assert.deepEqual(await f.updater.download(() => {}), { state: 'failed', reason: 'download' });
  assert.deepEqual(await first, { state: 'ready' });
});

test('busy names Device and never releases an unowned lock', async t => {
  const f = await fixture(t); await f.updater.check(); await f.updater.download(() => {}); f.state.busy = true;
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'busy', device: 'Studio' });
  assert.deepEqual(f.state.calls, [['POST', '/system/lock']]); assert.equal(f.state.launches.length, 0);
});

test('lock renewal failure releases owned lock without shutdown or launch', async t => {
  const f = await fixture(t); await f.updater.check(); await f.updater.download(() => {}); f.state.failRenew = true;
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'installer' });
  assert.deepEqual(f.state.calls.map(c => c[0]), ['POST', 'PUT', 'DELETE']); assert.equal(f.state.launches.length, 0);
});

test('successful handoff locks, renews, shuts down, launches verified bytes and quits without resolving', async t => {
  const f = await fixture(t); await f.updater.check(); await f.updater.download(() => {});
  let resolved = false; void f.updater.install().then(() => { resolved = true; });
  for (let n = 0; n < 100 && !f.state.quits; n++) await new Promise(resolve => setTimeout(resolve, 10));
  assert.equal(f.state.quits, 1); assert.equal(resolved, false);
  assert.deepEqual(f.state.calls, [['POST', '/system/lock'], ['PUT', '/system/lock/test-lock'], ['POST', '/system/shutdown']]);
  assert.deepEqual(f.state.launches, [{ data: f.state.data, args: INSTALLER_ARGS }]);
});

test('file changed after ready is refused and deleted before lock', async t => {
  const f = await fixture(t); await f.updater.check(); await f.updater.download(() => {});
  const path = join(f.dir, (await readdir(f.dir))[0], 'setup.exe'); await writeFile(path, 'tampered');
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'installer' });
  assert.equal(f.state.calls.length, 0); await assert.rejects(readFile(path));
});

test('real v0.1.0 installer verifies; one byte flipped is refused', { skip: !process.env.TOUCHDECK_REAL_UPDATE_ASSETS }, async t => {
  const root = process.env.TOUCHDECK_REAL_UPDATE_ASSETS;
  const dir = await mkdtemp(join(tmpdir(), 'touchdeck-real-update-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const path = join(root, 'TouchDeck-windows-x64-setup.exe');
  const signature = await readFile(`${path}.sig`, 'utf8');
  const { updater_public_key: key } = JSON.parse(await readFile(join(root, 'release-manifest.json'), 'utf8'));
  await verifyInstaller(path, signature, key);
  const bytes = await readFile(path); bytes[Math.floor(bytes.length / 2)] ^= 1;
  const changed = join(dir, 'changed.exe'); await writeFile(changed, bytes);
  await assert.rejects(verifyInstaller(changed, signature, key));
});

test('feed stays equal to the Tauri endpoint and seam types stay equal', async () => {
  const { UPDATE_FEED } = await import('../dist/updates/index.js');
  const tauri = JSON.parse(await readFile(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
  assert.equal(UPDATE_FEED, tauri.plugins.updater.endpoints[0]);
  const types = await readFile(new URL('../src/updates/types.ts', import.meta.url), 'utf8');
  const ui = await readFile(new URL('../../ui/src/companion/session.ts', import.meta.url), 'utf8');
  assert.ok(ui.startsWith(types));
});

test('oversized manifest fails and HTTP failures resolve', async t => {
  const f = await fixture(t);
  f.state.manifest.notes = 'x'.repeat(1024 * 1024);
  assert.deepEqual(await f.updater.check(), { state: 'failed' });
  f.state.status = 500;
  assert.deepEqual(await f.updater.check(), { state: 'failed' });
});

test('installer spawn failure returns failure and attempts owned unlock', async t => {
  const f = await fixture(t);
  const updater = new Updater({ ...f.options, launch: async () => { throw new Error('test launch failed'); } });
  await updater.check(); await updater.download(() => {});
  assert.deepEqual(await updater.install(), { state: 'failed', reason: 'installer' });
  assert.deepEqual(f.state.calls.at(-1), ['DELETE', '/system/lock/test-lock']);
  assert.equal(f.state.quits, 0);
});

test('signed fake installer executable receives passive restart flags', async t => {
  const { spawn } = await import('node:child_process');
  const f = await fixture(t);
  const marker = join(f.dir, 'launched.json');
  f.state.data = Buffer.from(`require('node:fs').writeFileSync(${JSON.stringify(marker)}, JSON.stringify(process.argv.slice(2)));`);
  f.state.manifest.platforms['windows-x86_64'].signature = f.signing.sign(f.state.data);
  const updater = new Updater({ ...f.options, launch: (path, args) => new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [path, ...args], { windowsHide: true, stdio: 'ignore' });
    child.once('error', reject); child.once('exit', code => code === 0 ? resolve() : reject(new Error('Fake installer failed')));
  }) });
  await updater.check(); assert.deepEqual(await updater.download(() => {}), { state: 'ready' });
  void updater.install();
  for (let n = 0; n < 200 && !f.state.quits; n++) await new Promise(resolve => setTimeout(resolve, 10));
  assert.equal(f.state.quits, 1);
  assert.deepEqual(JSON.parse(await readFile(marker, 'utf8')), ['/P', '/UPDATE', '/R']);
});


test('missing Companion resolves and permits another attempt', async t => {
  const f = await fixture(t);
  let available = false;
  const updater = new Updater({ ...f.options, system: () => {
    if (!available) throw new Error('Companion unavailable');
    return f.options.system();
  } });
  await updater.check(); await updater.download(() => {});
  assert.deepEqual(await updater.install(), { state: 'failed', reason: 'installer' });
  available = true; f.state.busy = true;
  assert.deepEqual(await updater.install(), { state: 'failed', reason: 'busy', device: 'Studio' });
});

test('signed version binds build metadata as well as precedence', async t => {
  const f = await fixture(t);
  f.state.manifest.version = '1.0.0+other';
  await f.updater.check();
  assert.deepEqual(await f.updater.download(() => {}), { state: 'failed', reason: 'signature' });
});
