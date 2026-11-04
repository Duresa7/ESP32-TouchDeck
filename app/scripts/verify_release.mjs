import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, mkdtemp, rm } from 'node:fs/promises';
import { createServer } from 'node:https';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Updater } from '../electron/dist/updates/index.js';

const [assets, key, cert] = process.argv.slice(2);
const name = 'OpenTouchDeck-windows-x64-setup.exe';
const installer = await readFile(join(assets, name));
assert.match(process.env.EXPECTED_SHA256 ?? '', /^[a-f0-9]{64}$/);
assert.equal(createHash('sha256').update(installer).digest('hex'), process.env.EXPECTED_SHA256);
const manifest = JSON.parse(await readFile(join(assets, 'release-manifest.json'), 'utf8'));
assert.equal(manifest.installer_sha256, process.env.EXPECTED_SHA256);
const publicKey = process.env.UPDATER_PUBLIC_KEY?.trim();
assert.ok(publicKey, 'Trusted updater public key is required');
assert.equal(manifest.updater_public_key, publicKey, 'Manifest updater public key differs from trusted key');
const latest = JSON.parse(await readFile(join(assets, 'latest.json'), 'utf8'));
assert.equal(latest.version, manifest.version);
const signature = (await readFile(join(assets, `${name}.sig`), 'utf8')).trim();
latest.platforms['windows-x86_64'].signature = signature;
const served = structuredClone(latest);
let tampered = false;
const server = createServer({ key: await readFile(key), cert: await readFile(cert) }, (req, res) => {
  if (req.url === '/latest.json') res.end(JSON.stringify(served));
  else {
    const bytes = Buffer.from(installer);
    if (tampered) bytes[bytes.length >> 1] ^= 1;
    res.end(bytes);
  }
});
await new Promise(resolve => server.listen(0, 'localhost', resolve));
const origin = `https://localhost:${server.address().port}`;
served.platforms['windows-x86_64'].url = `${origin}/setup.exe`;
const dir = await mkdtemp(join(tmpdir(), 'release-verify-'));
try {
  for (tampered of [false, true]) {
    const updater = new Updater({ publicKey, current: '0.0.0',
      runtimeDir: dir, feed: `${origin}/latest.json`,
      system: () => { throw new Error('Verification must not install'); },
      launch: () => { throw new Error('Verification must not launch'); }, quit: () => {} });
    assert.equal((await updater.check()).state, 'available');
    const result = await updater.download(() => {});
    assert.equal(result.state, tampered ? 'failed' : 'ready');
    if (tampered) assert.equal(result.reason, 'signature');
  }
  console.log('HTTPS updater accepted the signed installer and refused tampered bytes');
} finally {
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  await rm(dir, { recursive: true, force: true });
}
