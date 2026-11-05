// node cli.mjs sign --app-version X.Y.Z [--private-key-path <key>] <file>
//   Writes <file>.sig. The key comes from --private-key-path, TAURI_SIGNING_PRIVATE_KEY_PATH or
//   TAURI_SIGNING_PRIVATE_KEY, and its password from TAURI_SIGNING_PRIVATE_KEY_PASSWORD (unset means empty).
// node cli.mjs generate --write-keys <key> [--password <password>]
//   Writes a disposable test key to <key> and <key>.pub; never overwrites.
// Prints no key material, signature or password, even on failure.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { generateKeyPair, readSecretKey, signFile } from './minisign.mjs';

class UsageError extends Error {}

function privateKey(path, env) {
  const fromPath = path || env.TAURI_SIGNING_PRIVATE_KEY_PATH;
  const inline = env.TAURI_SIGNING_PRIVATE_KEY;
  if (fromPath && inline) throw new UsageError('Set the private key or its path, not both');
  if (fromPath) return readFileSync(fromPath, 'utf8');
  if (inline) return inline;
  throw new UsageError('No private key: set TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH');
}

async function main(argv, env) {
  const [command, ...rest] = argv;
  if (command === 'sign') {
    const { values, positionals } = parseArgs({ args: rest, allowPositionals: true,
      options: { 'app-version': { type: 'string' }, 'private-key-path': { type: 'string', short: 'f' } } });
    if (positionals.length !== 1) throw new UsageError('Name exactly one file to sign');
    const version = values['app-version'];
    if (!version) throw new UsageError('--app-version is required');
    const key = readSecretKey(privateKey(values['private-key-path'], env), env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? '');
    const [file] = positionals;
    writeFileSync(`${file}.sig`, await signFile(file, key, version));
    console.log(`Signed ${file}`);
    return;
  }
  if (command === 'generate') {
    const { values, positionals } = parseArgs({ args: rest, allowPositionals: true,
      options: { 'write-keys': { type: 'string', short: 'w' }, password: { type: 'string', short: 'p' } } });
    if (positionals.length || !values['write-keys']) throw new UsageError('--write-keys <path> is required');
    const path = values['write-keys'];
    if (existsSync(path) || existsSync(`${path}.pub`)) throw new UsageError('The key files already exist');
    const { secretKey, publicKey } = generateKeyPair(values.password ?? '');
    writeFileSync(path, secretKey, { flag: 'wx', mode: 0o600 });
    writeFileSync(`${path}.pub`, publicKey, { flag: 'wx' });
    console.log(`Wrote ${path} and ${path}.pub`);
    return;
  }
  throw new UsageError('Usage: cli.mjs sign|generate ...');
}

try {
  await main(process.argv.slice(2), process.env);
} catch (error) {
  const known = error instanceof UsageError || (error instanceof Error && !('code' in error) && error.constructor === Error);
  console.error(`release-signer: ${known ? error.message : `failed (${error?.code ?? error?.name ?? 'error'})`}`);
  process.exitCode = 1;
}
