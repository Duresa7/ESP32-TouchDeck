import {
  createCipheriv, createDecipheriv, createHash, createHmac,
  getDiffieHellman, timingSafeEqual,
} from 'node:crypto';

export const N_BYTES = getDiffieHellman('modp15').getPrime();
export const N = BigInt(`0x${N_BYTES.toString('hex')}`);
export const G = 5n;
export const LIMIT = 1n << 32n;
export const bytes = (value) => Buffer.from(value, 'hex');
export const concat = (...parts) => Buffer.concat(parts);
export const ascii = (value) => Buffer.from(value, 'ascii');
export const hash = (...parts) => createHash('sha256').update(concat(...parts)).digest();
export const hash512 = (...parts) => createHash('sha512').update(concat(...parts)).digest();
export const mac = (key, ...parts) => createHmac('sha256', key).update(concat(...parts)).digest();
export const extract = (salt, ikm) => mac(salt, ikm);
export const derive = (prk, label) => mac(prk, ascii(`touchdeck/net-v1/${label}`), Buffer.from([1]));
export const equal = (a, b) => a.length === b.length && timingSafeEqual(a, b);
export const integer = (value) => BigInt(`0x${value.toString('hex') || '00'}`);

export function requireThat(condition, reason) {
  if (!condition) throw new Error(reason);
}

export function fixed(value, size, name = 'field') {
  requireThat(Buffer.isBuffer(value) && value.length === size, `invalid ${name} length`);
  return value;
}

export function minimum(value) {
  requireThat(value >= 0n, 'negative integer');
  let hex = value.toString(16);
  if (hex.length % 2) hex = `0${hex}`;
  return bytes(hex);
}

export function pad(value) {
  const raw = minimum(value);
  requireThat(raw.length <= 384, 'integer overflow');
  return concat(Buffer.alloc(384 - raw.length), raw);
}

export function pow(base, exponent) {
  let result = 1n;
  base = ((base % N) + N) % N;
  while (exponent > 0n) {
    if (exponent & 1n) result = result * base % N;
    base = base * base % N;
    exponent >>= 1n;
  }
  return result;
}

export function u16(value) {
  const out = Buffer.alloc(2);
  out.writeUInt16BE(value);
  return out;
}

export function u32(value) {
  const out = Buffer.alloc(4);
  out.writeUInt32BE(value);
  return out;
}

export function u64(value) {
  const out = Buffer.alloc(8);
  out.writeBigUInt64BE(value);
  return out;
}

export function identity(deckId, appId) {
  fixed(deckId, 6, 'deck id');
  fixed(appId, 16, 'app id');
  return ascii(`touchdeck:${deckId.toString('hex')}:${appId.toString('hex')}`);
}

function privateExponent(value) {
  fixed(value, 32, 'private exponent');
  const number = integer(value);
  requireThat(number !== 0n, 'zero private exponent');
  return number;
}

export function publicValue(value) {
  fixed(value, 384, 'public value');
  const number = integer(value);
  requireThat(number > 0n && number < N, 'invalid public value');
  return number;
}

export const multiplier = () => integer(hash512(pad(N), pad(G)));

export function scrambling(A, B) {
  publicValue(A);
  publicValue(B);
  const value = integer(hash512(A, B));
  requireThat(value !== 0n, 'zero scrambling parameter');
  return value;
}

export function passwordExponent(I, code, salt) {
  requireThat(typeof code === 'string' && /^[0-9]{6}$/.test(code), 'invalid code');
  fixed(salt, 16, 'salt');
  return integer(hash512(salt, hash512(I, ascii(':'), ascii(code))));
}

export function clientPublic(a) {
  return pad(pow(G, privateExponent(a)));
}

export function serverPublic({ I, code, salt, b }) {
  const x = passwordExponent(I, code, salt);
  const v = pow(G, x);
  const B = pad((multiplier() * v + pow(G, privateExponent(b))) % N);
  publicValue(B);
  return { x, v, B };
}

export function clientSecret({ I, code, salt, a, B }) {
  const A = clientPublic(a);
  const bPublic = publicValue(B);
  const x = passwordExponent(I, code, salt);
  const u = scrambling(A, B);
  const base = ((bPublic - multiplier() * pow(G, x)) % N + N) % N;
  requireThat(base !== 0n, 'zero client base');
  const S = pow(base, privateExponent(a) + u * x);
  requireThat(S !== 0n, 'zero secret');
  return { A, x, u, S, K: hash512(minimum(S)) };
}

export function serverSecret({ I, code, salt, b, A }) {
  const { x, v, B } = serverPublic({ I, code, salt, b });
  const aPublic = publicValue(A);
  const u = scrambling(A, B);
  const S = pow(aPublic * pow(v, u) % N, privateExponent(b));
  requireThat(S !== 0n, 'zero secret');
  return { B, x, v, u, S, K: hash512(minimum(S)) };
}

const HANDSHAKE_SIZES = new Map([
  [0x11, 26], [0x12, 416], [0x13, 432], [0x14, 32], [0x15, 80],
  [0x16, 32], [0x17, 32], [0x20, 54], [0x21, 64], [0x22, 32], [0x23, 32], [0x7f, 1],
]);

function validateFields(kind, fields) {
  if (kind === 0x10) {
    requireThat(fields.length >= 18 && fields.length <= 81, 'invalid pair request');
    const name = fields.subarray(17);
    requireThat(fields[16] === name.length, 'invalid name length');
    const decoded = new TextDecoder('utf-8', { fatal: true }).decode(name);
    requireThat(!/\p{Cc}/u.test(decoded), 'control character in name');
    return;
  }
  requireThat(HANDSHAKE_SIZES.has(kind), 'unknown handshake kind');
  requireThat(fields.length === HANDSHAKE_SIZES.get(kind), 'invalid handshake length');
  if (kind === 0x7f) requireThat(fields[0] >= 1 && fields[0] <= 8, 'invalid error');
  if (kind === 0x11) {
    const remaining = fields.readUInt32BE(22);
    requireThat(remaining > 0 && remaining <= 120000, 'invalid window duration');
  }
}

export function handshake(kind, ...parts) {
  const fields = concat(...parts);
  validateFields(kind, fields);
  return concat(u16(fields.length + 2), Buffer.from([kind, 1]), fields);
}

export function parseHandshake(wire, expectedKind) {
  requireThat(wire.length >= 4 && wire.length <= 514, 'invalid handshake size');
  requireThat(wire.readUInt16BE() === wire.length - 2, 'invalid handshake prefix');
  requireThat(wire[3] === 1, 'invalid handshake version');
  const kind = wire[2];
  const fields = wire.subarray(4);
  validateFields(kind, fields);
  if (expectedKind !== undefined) requireThat(kind === expectedKind, 'unexpected handshake kind');
  return { kind, fields };
}

export function pairKeys(K, request, offer, start, challenge) {
  [request, offer, start, challenge].forEach((wire, i) => parseHandshake(wire, 0x10 + i));
  const transcript = hash(ascii('touchdeck/net-v1/pair\0'), request, offer, start, challenge);
  const prk = extract(transcript, fixed(K, 64, 'SRP key'));
  const client = derive(prk, 'pair/client');
  const deck = derive(prk, 'pair/deck');
  const wrap = derive(prk, 'pair/wrap');
  const commit = derive(prk, 'pair/commit');
  const clientProof = mac(client, transcript);
  const deckProof = mac(deck, transcript, clientProof);
  return { transcript, prk, client, deck, wrap, commit, clientProof, deckProof };
}

export function seal(key, nonce, plaintext, aad) {
  const cipher = createCipheriv('aes-256-gcm', fixed(key, 32), fixed(nonce, 12));
  cipher.setAAD(aad);
  return concat(cipher.update(plaintext), cipher.final(), cipher.getAuthTag());
}

export function open(key, nonce, sealed, aad) {
  requireThat(sealed.length >= 16, 'missing tag');
  const decipher = createDecipheriv('aes-256-gcm', fixed(key, 32), fixed(nonce, 12));
  decipher.setAAD(aad);
  decipher.setAuthTag(sealed.subarray(-16));
  return concat(decipher.update(sealed.subarray(0, -16)), decipher.final());
}

export function wrapKey(keys, ltk) {
  return seal(keys.wrap, Buffer.alloc(12), fixed(ltk, 32, 'LTK'),
    concat(keys.transcript, keys.clientProof, keys.deckProof));
}

export function acceptKey(keys, acceptance) {
  const { fields } = parseHandshake(acceptance, 0x15);
  requireThat(equal(fields.subarray(0, 32), keys.deckProof), 'bad deck proof');
  return fixed(open(keys.wrap, Buffer.alloc(12), fields.subarray(32),
    concat(keys.transcript, keys.clientProof, keys.deckProof)), 32, 'LTK');
}

export const commitProof = (keys, ltk) => mac(keys.commit, ascii('commit'), keys.transcript, hash(ltk));
export const storedProof = (keys, ltk) => mac(keys.commit, ascii('stored'), keys.transcript, hash(ltk));

export function sessionKeys({ ltk, deckId, appId, clientNonce, deckNonce }) {
  const transcript = hash(ascii('touchdeck/net-v1/session\0'), fixed(deckId, 6),
    fixed(appId, 16), fixed(clientNonce, 32), fixed(deckNonce, 32));
  const prk = extract(transcript, fixed(ltk, 32, 'LTK'));
  const deck = derive(prk, 'session/deck');
  const client = derive(prk, 'session/client');
  const ready = derive(prk, 'session/ready');
  const c2d = derive(prk, 'session/c2d');
  const d2c = derive(prk, 'session/d2c');
  const deckProof = mac(deck, transcript);
  const clientProof = mac(client, transcript, deckProof);
  const readyProof = mac(ready, transcript, clientProof);
  return { transcript, prk, deck, client, ready, c2d, d2c, deckProof, clientProof, readyProof };
}

export function crc16(body) {
  let crc = 0xffff;
  for (const byte of body) {
    crc ^= byte << 8;
    for (let bit = 0; bit < 8; bit++) crc = ((crc << 1) ^ (crc & 0x8000 ? 0x1021 : 0)) & 0xffff;
  }
  return crc;
}

export function frame(payload, { type = 1, seq = 1, flags = 0 } = {}) {
  if (!Buffer.isBuffer(payload)) payload = Buffer.from(JSON.stringify(payload));
  requireThat(payload.length <= 1024, 'payload too large');
  const body = Buffer.alloc(payload.length + 8);
  body.set([1, type, seq, flags]);
  body.writeUInt16LE(payload.length, 4);
  body.set(payload, 6);
  body.writeUInt16LE(crc16(body.subarray(0, -2)), body.length - 2);
  validateFrame(body);
  return body;
}

export function validateFrame(body) {
  requireThat(body.length >= 8 && body.length <= 1032, 'invalid frame size');
  const [version, type, seq, flags] = body;
  const length = body.readUInt16LE(4);
  requireThat(version === 1 && type >= 1 && type <= 3 && flags <= 2, 'invalid frame header');
  requireThat(flags !== 2 || seq === 0, 'invalid event sequence');
  requireThat(length <= 1024 && body.length === length + 8, 'invalid payload length');
  requireThat(type !== 2 || length >= 3, 'invalid chunk length');
  requireThat(type !== 3 || length === 3, 'invalid ack length');
  requireThat(crc16(body.subarray(0, -2)) === body.readUInt16LE(body.length - 2), 'invalid CRC');
  return { type, seq, flags, payload: body.subarray(6, -2) };
}

export function recordParameters(direction, counter, length) {
  requireThat(direction === 0 || direction === 1, 'invalid direction');
  requireThat(typeof counter === 'bigint' && counter >= 0n && counter < LIMIT, 'counter exhausted');
  requireThat(length >= 24 && length <= 1048, 'invalid record length');
  return {
    nonce: concat(Buffer.alloc(4), u64(counter)),
    aad: concat(ascii('TDN1'), Buffer.from([direction]), u64(counter), u16(length)),
  };
}

export class RecordSender {
  constructor(key, direction) {
    this.key = Buffer.from(fixed(key, 32));
    this.direction = direction;
    this.counter = 0n;
    this.closed = false;
  }

  seal(body) {
    requireThat(!this.closed, 'sender closed');
    try {
      validateFrame(body);
      const length = body.length + 16;
      const { nonce, aad } = recordParameters(this.direction, this.counter, length);
      const wire = concat(u16(length), seal(this.key, nonce, body, aad));
      this.counter++;
      return wire;
    } catch (error) {
      this.close();
      throw error;
    }
  }

  close() {
    this.closed = true;
    this.key.fill(0);
  }
}

export class RecordReceiver {
  constructor(key, direction) {
    this.key = Buffer.from(fixed(key, 32));
    this.direction = direction;
    this.counter = 0n;
    this.closed = false;
  }

  open(wire) {
    requireThat(!this.closed, 'receiver closed');
    try {
      requireThat(wire.length >= 26 && wire.length <= 1050, 'invalid record size');
      const length = wire.readUInt16BE();
      requireThat(length === wire.length - 2, 'invalid record prefix');
      const { nonce, aad } = recordParameters(this.direction, this.counter, length);
      const body = open(this.key, nonce, wire.subarray(2), aad);
      validateFrame(body);
      this.counter++;
      return body;
    } catch (error) {
      this.close();
      throw error;
    }
  }

  close() {
    this.closed = true;
    this.key.fill(0);
  }
}

export class PairingWindow {
  constructor(now) {
    this.expires = now + 120000;
    this.attemptDeadline = null;
    this.failures = 0;
    this.state = 'waiting';
  }

  check(now) {
    if (now >= this.expires || (this.attemptDeadline !== null && now >= this.attemptDeadline)) {
      this.state = 'closed';
    }
    requireThat(this.state !== 'closed', 'pairing closed');
  }

  expect(state) {
    if (this.state !== state) {
      this.state = 'closed';
      throw new Error('unexpected pairing transition');
    }
  }

  start(now) {
    this.check(now);
    this.expect('waiting');
    this.state = 'proving';
    this.attemptDeadline = now + 10000;
  }

  proof(valid, now) {
    this.check(now);
    this.expect('proving');
    if (valid) this.state = 'committing';
    else {
      this.failures++;
      this.state = this.failures === 3 ? 'closed' : 'waiting';
      this.attemptDeadline = null;
    }
  }

  commit(now) {
    this.check(now);
    this.expect('committing');
    this.state = 'closed';
  }

  cancel() {
    this.state = 'closed';
  }
}
