import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { generateKeyPairSync, randomBytes, sign } from 'node:crypto';
import { writeFile, rm } from 'node:fs/promises';

const require = createRequire(import.meta.url);
function run(script, args = [], env = process.env) {
  const result = spawnSync(process.execPath, [script, ...args], { stdio: 'inherit', env, windowsHide: true });
  if (result.status !== 0) throw new Error(`Command failed: ${script}`);
}
function build(env) {
  run('build-key.mjs', [], env);
  run(require.resolve('typescript/bin/tsc'));
}
const playwright = require.resolve('@playwright/test/cli');
const sourceEnv = { ...process.env, TOUCHDECK_UPDATER_PUBKEY: '' };
build(sourceEnv);
run(playwright, ['test', '--grep-invert', 'authorized top frame reaches']);
const { publicKey, privateKey } = generateKeyPairSync('ed25519');
const id = randomBytes(8);
const key = Buffer.concat([Buffer.from('Ed'), id, publicKey.export({ type: 'spki', format: 'der' }).subarray(-32)]);
const encodedKey = Buffer.from(`untrusted comment: test\n${key.toString('base64')}\n`).toString('base64');
const data = randomBytes(4096);
const sig = sign(null, data, privateKey);
const comment = 'timestamp:1\tfile:setup.exe\tversion:99.0.0';
const global = sign(null, Buffer.concat([sig, Buffer.from(comment)]), privateKey);
const packet = Buffer.concat([Buffer.from('Ed'), id, sig]);
const signature = Buffer.from(`untrusted comment: test\n${packet.toString('base64')}\ntrusted comment: ${comment}\n${global.toString('base64')}\n`).toString('base64');
try {
  build({ ...process.env, TOUCHDECK_UPDATER_PUBKEY: encodedKey });
  await writeFile(new URL('./dist/e2e-update.json', import.meta.url), JSON.stringify({ data: data.toString('base64'), signature }));
  run(playwright, ['test', 'updates.spec.ts']);
} finally {
  await rm(new URL('./dist/e2e-update.json', import.meta.url), { force: true });
  build(sourceEnv);
}
