import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto';
import { createServer } from 'node:http';
import { mkdtemp, readdir, readFile, rm, symlink, truncate, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setFlagsFromString } from 'node:v8';
import { runInNewContext } from 'node:vm';
import { blake2b } from '@noble/hashes/blake2.js';
import { Updater, feedSettings, INSTALLER_ARGS } from '../dist/updates/index.js';
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
  const options = { publicKey: signing.key, current: '0.2.0', runtimeDir: dir,
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

test('a download survives garbage collection before its body is read', async t => {
  setFlagsFromString('--expose-gc');
  const gc = runInNewContext('gc');
  const f = await fixture(t); await f.updater.check();
  let collecting = true;
  const collector = (async () => { while (collecting) { gc(); await new Promise(resolve => setImmediate(resolve)); } })();
  try { assert.deepEqual(await f.updater.download(() => {}), { state: 'ready' }); }
  finally { collecting = false; await collector; }
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
  assert.deepEqual(await readdir(f.dir), []);
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
  assert.deepEqual(await readdir(f.dir), []);
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
  assert.deepEqual(await readdir(f.dir), []);
});

test('noble BLAKE2b-512 matches the RFC 7693 vectors and streams like one-shot', () => {
  const vectors = {
    '': '786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce',
    abc: 'ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923',
  };
  for (const [text, digest] of Object.entries(vectors)) {
    assert.equal(Buffer.from(blake2b.create({ dkLen: 64 }).update(Buffer.from(text)).digest()).toString('hex'), digest);
  }
  const data = randomBytes(300_000);
  const hash = blake2b.create({ dkLen: 64 });
  for (let at = 0; at < data.length; at += 65_536) hash.update(data.subarray(at, at + 65_536));
  assert.deepEqual(Buffer.from(hash.digest()), createHash('blake2b512').update(data).digest());
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

test('feed stays on the release endpoint and seam types stay equal', async () => {
  const { UPDATE_FEED } = await import('../dist/updates/index.js');
  assert.equal(UPDATE_FEED, 'https://github.com/Duresa7/ESP32-TouchDeck/releases/latest/download/latest.json');
  const types = await readFile(new URL('../src/updates/types.ts', import.meta.url), 'utf8');
  const ui = await readFile(new URL('../../ui/src/companion/session.ts', import.meta.url), 'utf8');
  const declarations = text => text.replace(/\/\/[^\n]*/g, '').replace(/\s+/g, ' ').trim();
  assert.ok(declarations(ui).includes(declarations(types)));
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
  assert.deepEqual(await readdir(f.dir), []);
});

test('signed fake installer executable receives silent update and restart flags', async t => {
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
  assert.deepEqual(JSON.parse(await readFile(marker, 'utf8')), ['/S', '--updated', '--force-run']);
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
  assert.deepEqual(await readdir(f.dir), []);
  available = true; f.state.busy = true;
  await updater.download(() => {});
  assert.deepEqual(await updater.install(), { state: 'failed', reason: 'busy', device: 'Studio' });
});

test('signed version binds build metadata as well as precedence', async t => {
  const f = await fixture(t);
  f.state.manifest.version = '1.0.0+other';
  await f.updater.check();
  assert.deepEqual(await f.updater.download(() => {}), { state: 'failed', reason: 'signature' });
  assert.deepEqual(await readdir(f.dir), []);
});

for (const step of ['lock', 'renew', 'shutdown']) {
  for (const mutation of ['rewrite', 'replace']) test(`refuses ${mutation} during ${step} and deletes staging`, async t => {
    const f = await fixture(t);
    const original = f.options.system;
    f.options.system = () => async (method, path) => {
      if ((step === 'lock' && path === '/system/lock') || (step === 'renew' && method === 'PUT') ||
          (step === 'shutdown' && path === '/system/shutdown')) {
        const installer = join(f.dir, (await readdir(f.dir))[0], 'setup.exe');
        if (mutation === 'replace') await rm(installer);
        await writeFile(installer, 'untrusted bytes');
      }
      return original()(method, path);
    };
    await f.updater.check(); await f.updater.download(() => {});
    assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'installer' });
    assert.deepEqual(await readdir(f.dir), []);
    assert.equal(f.state.launches.length, 0);
    assert.equal(f.state.quits, 0);
    assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'installer' });
  });
}

for (const failure of ['lock', 'shutdown', 'oversized']) test(`cleans staging on ${failure} install failure`, async t => {
  const f = await fixture(t);
  const original = f.options.system;
  f.options.system = () => async (method, path) => path === `/system/${failure}`
    ? { status: 503, body: null } : original()(method, path);
  await f.updater.check(); await f.updater.download(() => {});
  if (failure === 'oversized') await truncate(join(f.dir, (await readdir(f.dir))[0], 'setup.exe'), 256 * 1024 * 1024 + 1);
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'installer' });
  assert.deepEqual(await readdir(f.dir), []);
  assert.equal(f.state.launches.length, 0);
});

test('busy retains the ready installer for a successful retry', async t => {
  const f = await fixture(t); await f.updater.check(); await f.updater.download(() => {});
  const folders = await readdir(f.dir); f.state.busy = true;
  assert.equal((await f.updater.install()).reason, 'busy');
  assert.deepEqual(await readdir(f.dir), folders);
  assert.deepEqual(await readFile(join(f.dir, folders[0], 'setup.exe')), f.state.data);
  f.state.busy = false; void f.updater.install();
  for (let n = 0; n < 100 && !f.state.quits; n++) await new Promise(resolve => setTimeout(resolve, 10));
  assert.equal(f.state.quits, 1);
});

test('replacement download removes the previous staging folder', async t => {
  const f = await fixture(t); await f.updater.check(); await f.updater.download(() => {});
  const first = (await readdir(f.dir))[0];
  assert.deepEqual(await f.updater.download(() => {}), { state: 'ready' });
  const next = await readdir(f.dir); assert.equal(next.length, 1); assert.notEqual(next[0], first);
});

test('startup removes stale update folders and preserves other runtime files', async t => {
  const f = await fixture(t); await f.updater.check(); await f.updater.download(() => {});
  await writeFile(join(f.dir, 'companion.json'), '{}');
  const updater = new Updater(f.options);
  await updater.check(); await updater.download(() => {});
  assert.equal((await readdir(f.dir)).filter(name => name.startsWith('update-')).length, 1);
  assert.equal(await readFile(join(f.dir, 'companion.json'), 'utf8'), '{}');
});

test('reparse-point staging is refused before launch and its link is deleted', async t => {
  const f = await fixture(t); await f.updater.check(); await f.updater.download(() => {});
  const folder = join(f.dir, (await readdir(f.dir))[0]);
  const target = await mkdtemp(join(tmpdir(), 'touchdeck-link-target-'));
  t.after(() => rm(target, { recursive: true, force: true }));
  await rm(folder, { recursive: true });
  await writeFile(join(target, 'setup.exe'), f.state.data);
  try { await symlink(target, folder, process.platform === 'win32' ? 'junction' : 'dir'); }
  catch (error) { if (['EPERM', 'EACCES', 'ENOTSUP'].includes(error.code)) return t.skip(`OS cannot create directory link: ${error.code}`); throw error; }
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'installer' });
  assert.deepEqual(await readdir(f.dir), []);
  await assert.rejects(readFile(join(folder, 'setup.exe')));
  assert.equal(f.state.launches.length, 0);
  assert.deepEqual(await readFile(join(target, 'setup.exe')), f.state.data);
});

test('source build ignores stray runtime key files', async t => {
  const { UPDATER_PUBLIC_KEY } = await import('../dist/updates/key.gen.js');
  const f = await fixture(t);
  const loose = new URL('../dist/updater-key.json', import.meta.url);
  await writeFile(loose, JSON.stringify({ publicKey: f.signing.key }));
  t.after(() => rm(loose, { force: true }));
  assert.equal(UPDATER_PUBLIC_KEY, '');
  assert.deepEqual(await new Updater({ ...f.options, publicKey: UPDATER_PUBLIC_KEY }).check(), { state: 'disabled' });
});

test('failed download after release disappears removes the ready installer', async t => {
  const f = await fixture(t); await f.updater.check(); await f.updater.download(() => {});
  f.state.manifest.version = '0.2.0'; await f.updater.check();
  assert.deepEqual(await f.updater.download(() => {}), { state: 'failed', reason: 'download' });
  assert.deepEqual(await readdir(f.dir), []);
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'installer' });
});

test('staging changed to a junction during shutdown is refused at launch', async t => {
  const f = await fixture(t);
  const target = await mkdtemp(join(tmpdir(), 'touchdeck-link-target-'));
  t.after(() => rm(target, { recursive: true, force: true }));
  const probe = join(f.dir, 'probe');
  try { await symlink(target, probe, process.platform === 'win32' ? 'junction' : 'dir'); }
  catch (error) { if (['EPERM', 'EACCES', 'ENOTSUP'].includes(error.code)) return t.skip(`OS cannot create directory link: ${error.code}`); throw error; }
  await rm(probe);
  const original = f.options.system;
  f.options.system = () => async (method, path) => {
    if (path === '/system/shutdown') {
      const folder = join(f.dir, (await readdir(f.dir))[0]);
      await rm(folder, { recursive: true });
      await writeFile(join(target, 'setup.exe'), f.state.data);
      await symlink(target, folder, process.platform === 'win32' ? 'junction' : 'dir');
    }
    return original()(method, path);
  };
  await f.updater.check(); await f.updater.download(() => {});
  assert.deepEqual(await f.updater.install(), { state: 'failed', reason: 'installer' });
  assert.deepEqual(await readdir(f.dir), []);
  assert.equal(f.state.launches.length, 0);
});

test('runtime junction is refused before writing any installer', async t => {
  const f = await fixture(t);
  const link = join(f.dir, 'runtime');
  const target = await mkdtemp(join(tmpdir(), 'touchdeck-runtime-target-'));
  t.after(() => rm(target, { recursive: true, force: true }));
  try { await symlink(target, link, process.platform === 'win32' ? 'junction' : 'dir'); }
  catch (error) { if (['EPERM', 'EACCES', 'ENOTSUP'].includes(error.code)) return t.skip(`OS cannot create directory link: ${error.code}`); throw error; }
  const updater = new Updater({ ...f.options, runtimeDir: link });
  await updater.check();
  assert.deepEqual(await updater.download(() => {}), { state: 'failed', reason: 'download' });
  assert.deepEqual(await readdir(target), []);
});
