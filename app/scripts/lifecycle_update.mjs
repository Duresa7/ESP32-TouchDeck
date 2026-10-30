import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [scenario, current, dir, resultPath] = process.argv.slice(2);
if (!['good', 'tampered'].includes(scenario) || !current || !dir || !resultPath) {
  throw new Error('Usage: lifecycle_update.mjs <good|tampered> <current version> <lifecycle dir> <result.json>');
}
const dist = new URL('../electron/dist/', import.meta.url);
const { Updater, INSTALLER_ARGS } = await import(new URL('updates/index.js', dist).href);
const { connect, runtimePath } = await import(new URL('companion.js', dist).href);

const next = JSON.parse(await readFile(join(dir, 'next.json'), 'utf8'));
const installer = await readFile(join(dir, next.installer));
const served = scenario === 'tampered' ? Buffer.from(installer) : installer;
if (scenario === 'tampered') served[served.length >> 1] ^= 0x01;

const server = createServer((req, res) => {
  if (req.url === '/latest.json') {
    const url = `http://127.0.0.1:${server.address().port}/setup.exe`;
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify({ version: next.version, notes: 'lifecycle', pub_date: null,
      platforms: { 'windows-x86_64': { signature: next.signature, url, size: served.length } } }));
  } else if (req.url === '/setup.exe') {
    res.setHeader('Content-Length', served.length);
    res.end(served);
  } else {
    res.statusCode = 404;
    res.end();
  }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));

const result = { scenario, args: INSTALLER_ARGS };
const finish = async () => {
  server.close();
  await writeFile(resultPath, JSON.stringify(result, null, 2));
};

const staging = join(tmpdir(), 'td-lifecycle-staging');
await mkdir(staging, { recursive: true });
const session = await connect(runtimePath(), () => { throw new Error('The installed Companion is not running'); });
const updater = new Updater({
  publicKey: next.updater_public_key, current, runtimeDir: staging,
  feed: `http://127.0.0.1:${server.address().port}/latest.json`, allowLoopback: true,
  system: () => async (method, path) => {
    const res = await fetch(`${session.origin}/api/v1${path}`, {
      method, redirect: 'error', signal: AbortSignal.timeout(5000),
      headers: { Authorization: `Bearer ${session.token}`, 'Content-Type': 'application/json' },
    });
    return { status: res.status, body: await res.json().catch(() => null) };
  },
  launch: (path, args) => new Promise((resolve, reject) => {
    const child = spawn(path, args, { detached: true, stdio: 'ignore', windowsHide: true });
    child.once('error', reject);
    child.once('spawn', () => { result.installer_pid = child.pid; child.unref(); resolve(); });
  }),
  quit: () => { result.quit = true; void finish().then(() => process.exit(0)); },
});
result.check = await updater.check();
result.download = await updater.download(() => {});
if (scenario === 'good' && result.download.state === 'ready') {
  result.install = await updater.install();
}
await finish();
