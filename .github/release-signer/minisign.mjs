import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes, scryptSync, sign, timingSafeEqual, verify } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { basename } from 'node:path';

const ALG = Buffer.from('Ed');
const ALG_PREHASHED = Buffer.from('ED');
const KDF_ALG = Buffer.from('Sc');
const CHK_ALG = Buffer.from('B2');
const SECRET_KEY_BYTES = 158;
const ENCRYPTED_BYTES = 8 + 64 + 32;
const OPSLIMIT = 1_048_576n;
const MEMLIMIT = 33_554_432n;
const MEMLIMIT_MAX = 1_073_741_824n;
const N_LOG2_MAX = 20n;
const PKCS8_SEED = Buffer.from('302e020100300506032b657004220420', 'hex');
const SPKI_KEY = Buffer.from('302a300506032b6570032100', 'hex');
const SIGNATURE_COMMENT = 'signature from tauri secret key';
const SINGLE_LINE = /[\t\r\n]/;

const MASK = (1n << 64n) - 1n;
const IV = [0x6a09e667f3bcc908n, 0xbb67ae8584caa73bn, 0x3c6ef372fe94f82bn, 0xa54ff53a5f1d36f1n,
  0x510e527fade682d1n, 0x9b05688c2b3e6c1fn, 0x1f83d9abfb41bd6bn, 0x5be0cd19137e2179n];
const SIGMA = [
  [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15], [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
  [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4], [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
  [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13], [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
  [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11], [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
  [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5], [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

const rotr = (x, n) => ((x >> n) | (x << (64n - n))) & MASK;

function compress(h, block, counter, last) {
  const m = Array.from({ length: 16 }, (_, i) => block.readBigUInt64LE(i * 8));
  const v = [...h, ...IV];
  v[12] ^= counter & MASK;
  v[13] ^= counter >> 64n;
  if (last) v[14] ^= MASK;
  const g = (a, b, c, d, x, y) => {
    v[a] = (v[a] + v[b] + x) & MASK; v[d] = rotr(v[d] ^ v[a], 32n);
    v[c] = (v[c] + v[d]) & MASK; v[b] = rotr(v[b] ^ v[c], 24n);
    v[a] = (v[a] + v[b] + y) & MASK; v[d] = rotr(v[d] ^ v[a], 16n);
    v[c] = (v[c] + v[d]) & MASK; v[b] = rotr(v[b] ^ v[c], 63n);
  };
  for (let round = 0; round < 12; round++) {
    const s = SIGMA[round % 10];
    g(0, 4, 8, 12, m[s[0]], m[s[1]]); g(1, 5, 9, 13, m[s[2]], m[s[3]]);
    g(2, 6, 10, 14, m[s[4]], m[s[5]]); g(3, 7, 11, 15, m[s[6]], m[s[7]]);
    g(0, 5, 10, 15, m[s[8]], m[s[9]]); g(1, 6, 11, 12, m[s[10]], m[s[11]]);
    g(2, 7, 8, 13, m[s[12]], m[s[13]]); g(3, 4, 9, 14, m[s[14]], m[s[15]]);
  }
  for (let i = 0; i < 8; i++) h[i] ^= v[i] ^ v[i + 8];
}

/** Unkeyed BLAKE2b (RFC 7693) with any digest length; minisign's key checksum is BLAKE2b-256, which node:crypto lacks. */
export function blake2b(data, length) {
  if (!Number.isInteger(length) || length < 1 || length > 64) throw new RangeError('BLAKE2b length must be 1 to 64 bytes');
  const h = IV.slice();
  h[0] ^= 0x01010000n ^ BigInt(length);
  const blocks = Math.max(1, Math.ceil(data.length / 128));
  for (let i = 0; i < blocks; i++) {
    const block = Buffer.alloc(128);
    data.copy(block, 0, i * 128, Math.min(data.length, (i + 1) * 128));
    const last = i === blocks - 1;
    compress(h, block, BigInt(last ? data.length : (i + 1) * 128), last);
  }
  const out = Buffer.alloc(64);
  h.forEach((word, i) => out.writeBigUInt64LE(word, i * 8));
  return out.subarray(0, length);
}

function decodeBase64(text, what) {
  const value = text.trim();
  if (!value || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) throw new Error(`${what} is not valid base64`);
  return Buffer.from(value, 'base64');
}

const wrap = text => Buffer.from(text, 'utf8').toString('base64');

/** minisign's raw_scrypt_params: the scrypt cost that libsodium's opslimit and memlimit describe. */
export function scryptParams(opslimit, memlimit) {
  if (memlimit > MEMLIMIT_MAX) throw new Error('The private key asks for too much scrypt memory');
  const ops = opslimit < 32768n ? 32768n : opslimit;
  const r = 8n;
  const fit = maxn => { let log = 1n; while (log < 63n && (1n << log) <= maxn / 2n) log++; return log; };
  let log, p;
  if (ops < memlimit / 32n) {
    log = fit(ops / (r * 4n));
    p = 1n;
  } else {
    log = fit(memlimit / (r * 128n));
    const maxrp = BigInt.asUintN(32, (ops / 4n) / (1n << log));
    p = (maxrp < 0x3fffffffn ? maxrp : 0x3fffffffn) / r;
  }
  if (log > N_LOG2_MAX) throw new Error('The private key asks for too much scrypt work');
  const N = 2 ** Number(log);
  return { N, r: Number(r), p: Number(p), maxmem: 256 * N * Number(r) };
}

function keyStream(password, salt, opslimit, memlimit) {
  return scryptSync(Buffer.from(password, 'utf8'), salt, ENCRYPTED_BYTES, scryptParams(opslimit, memlimit));
}

const xor = (data, stream) => Buffer.from(data.map((byte, i) => byte ^ stream[i]));

/** The base64-wrapped public key box Tauri wrote next to a secret key (`<key>.pub`). */
export function publicKeyBox(keyId, rawPublicKey) {
  const id = Buffer.from(keyId).reverse().toString('hex').toUpperCase();
  return wrap(`untrusted comment: minisign public key: ${id}\n${Buffer.concat([ALG, keyId, rawPublicKey]).toString('base64')}\n`);
}

/** Decrypts a base64-wrapped, password-protected minisign secret key, as `tauri signer sign` read it. */
export function readSecretKey(wrapped, password = '') {
  const lines = decodeBase64(wrapped, 'The private key').toString('utf8').split(/\r?\n/);
  const raw = decodeBase64(lines[1] ?? '', 'The private key');
  if (raw.length !== SECRET_KEY_BYTES) throw new Error('The private key has the wrong length');
  const alg = raw.subarray(0, 2);
  if (!alg.equals(ALG) || !raw.subarray(2, 4).equals(KDF_ALG) || !raw.subarray(4, 6).equals(CHK_ALG)) {
    throw new Error('The private key is not a password-protected minisign Ed25519 key');
  }
  const salt = raw.subarray(6, 38);
  const plain = xor(raw.subarray(54), keyStream(password, salt, raw.readBigUInt64LE(38), raw.readBigUInt64LE(46)));
  const keyId = plain.subarray(0, 8), secret = plain.subarray(8, 72), checksum = plain.subarray(72);
  if (!timingSafeEqual(blake2b(Buffer.concat([alg, keyId, secret]), 32), checksum)) throw new Error('Wrong password for the private key');
  const privateKey = createPrivateKey({ key: Buffer.concat([PKCS8_SEED, secret.subarray(0, 32)]), format: 'der', type: 'pkcs8' });
  const publicKey = createPublicKey(privateKey);
  const rawPublicKey = publicKey.export({ format: 'der', type: 'spki' }).subarray(SPKI_KEY.length);
  if (!rawPublicKey.equals(secret.subarray(32))) throw new Error('The private key does not match its public half');
  return { keyId: Buffer.from(keyId), privateKey, publicKey, rawPublicKey, publicKeyBox: publicKeyBox(keyId, rawPublicKey) };
}

/** A new password-protected key pair with minisign's default scrypt cost. Disposable test keys only. */
export function generateKeyPair(password = '', comment = 'Open TouchDeck disposable test-only updater key') {
  if (SINGLE_LINE.test(comment)) throw new Error('The key comment must be one line');
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const seed = privateKey.export({ format: 'der', type: 'pkcs8' }).subarray(PKCS8_SEED.length);
  const rawPublicKey = publicKey.export({ format: 'der', type: 'spki' }).subarray(SPKI_KEY.length);
  const keyId = randomBytes(8), salt = randomBytes(32);
  const secret = Buffer.concat([seed, rawPublicKey]);
  const checksum = blake2b(Buffer.concat([ALG, keyId, secret]), 32);
  const limits = Buffer.alloc(16);
  limits.writeBigUInt64LE(OPSLIMIT, 0);
  limits.writeBigUInt64LE(MEMLIMIT, 8);
  const encrypted = xor(Buffer.concat([keyId, secret, checksum]), keyStream(password, salt, OPSLIMIT, MEMLIMIT));
  const raw = Buffer.concat([ALG, KDF_ALG, CHK_ALG, salt, limits, encrypted]);
  return { secretKey: wrap(`untrusted comment: ${comment}\n${raw.toString('base64')}\n`), publicKey: publicKeyBox(keyId, rawPublicKey) };
}

/**
 * The base64-wrapped `.sig` text for `path`: BLAKE2b-512 prehashed Ed25519 (`ED`) with the trusted
 * comment `timestamp:<s>\tfile:<name>\tversion:<version>`, as `tauri signer sign --app-version` wrote it.
 */
export async function signFile(path, key, version, { timestamp = Math.floor(Date.now() / 1000) } = {}) {
  const name = basename(path);
  if (typeof version !== 'string' || !version) throw new Error('An app version is required');
  if (SINGLE_LINE.test(version)) throw new Error('The app version cannot contain a tab or newline');
  if (SINGLE_LINE.test(name)) throw new Error('The file name cannot contain a tab or newline');
  if (!Number.isSafeInteger(timestamp) || timestamp < 0) throw new Error('Invalid timestamp');
  const hash = createHash('blake2b512');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  const digest = hash.digest();
  const signature = sign(null, digest, key.privateKey);
  const comment = `timestamp:${timestamp}\tfile:${name}\tversion:${version}`;
  const signed = Buffer.concat([signature, Buffer.from(comment, 'utf8')]);
  const global = sign(null, signed, key.privateKey);
  if (!verify(null, digest, key.publicKey, signature) || !verify(null, signed, key.publicKey, global)) {
    throw new Error('The new signature did not verify');
  }
  const packet = Buffer.concat([ALG_PREHASHED, key.keyId, signature]);
  return wrap(`untrusted comment: ${SIGNATURE_COMMENT}\n${packet.toString('base64')}\ntrusted comment: ${comment}\n${global.toString('base64')}\n`);
}
