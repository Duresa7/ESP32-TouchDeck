// Reference for schema/protocol/net-v2.md. It reuses the SRP arithmetic,
// hashes, AES-GCM and link body checks from the net-v1 reference and adds the
// net-v2 item version, labels, the X25519 session exchange, TDN2 records and
// BLE fragmentation. It is a test oracle, not a production crypto backend.
import { createPrivateKey, createPublicKey, diffieHellman } from 'node:crypto';
import {
  ascii, concat, equal, fixed, hash, mac, extract, requireThat, seal, open,
  u16, u64, validateFrame, LIMIT,
} from './protocol.mjs';

export const VERSION = 2;
export const PORT = 47820;
export const MIN_HANDSHAKE_BODY = 3;
export const MAX_HANDSHAKE_BODY = 512;
export const MAX_RECORD_BODY = 1048;
export const MIN_RECORD_BODY = 24;
export const MAX_ITEM = MAX_RECORD_BODY + 2;
export const ATT_MTU_MIN = 23;
export const ATT_MTU_MAX = 517;
export const MIN_INITIAL_CREDIT = MAX_ITEM;

export const gatt = {
  service: '6198f058-afef-4469-b47f-07315672e2dd',
  rx: '6198f059-afef-4469-b47f-07315672e2dd',
  tx: '6198f05a-afef-4469-b47f-07315672e2dd',
  flow: '6198f05b-afef-4469-b47f-07315672e2dd',
};

export const derive = (prk, label) => mac(prk, ascii(`touchdeck/net-v2/${label}`), Buffer.from([1]));

const SIZES = new Map([
  [0x11, 26], [0x12, 416], [0x13, 432], [0x14, 32], [0x15, 80],
  [0x16, 32], [0x17, 32], [0x20, 54], [0x21, 64], [0x22, 32], [0x23, 32], [0x7f, 1],
]);

function validateFields(kind, fields) {
  if (kind === 0x10) {
    requireThat(fields.length >= 18 && fields.length <= 81, 'invalid pair request');
    const name = fields.subarray(17);
    requireThat(fields[16] === name.length, 'invalid name length');
    const decoded = new TextDecoder('utf-8', { fatal: true }).decode(name);
    requireThat(!/[\p{Cc}\u202a-\u202e\u2066-\u2069]/u.test(decoded), 'control character in name');
    return;
  }
  requireThat(SIZES.has(kind), 'unknown handshake kind');
  requireThat(fields.length === SIZES.get(kind), 'invalid handshake length');
  if (kind === 0x7f) requireThat(fields[0] >= 1 && fields[0] <= 9, 'invalid error');
  if (kind === 0x11) {
    const remaining = fields.readUInt32BE(22);
    requireThat(remaining > 0 && remaining <= 120000, 'invalid window duration');
  }
}

export function handshake(kind, ...parts) {
  const fields = concat(...parts);
  validateFields(kind, fields);
  return concat(u16(fields.length + 2), Buffer.from([kind, VERSION]), fields);
}

export function parseHandshake(wire, expectedKind) {
  requireThat(wire.length >= 4 && wire.length <= MAX_HANDSHAKE_BODY + 2, 'invalid handshake size');
  requireThat(wire.readUInt16BE() === wire.length - 2, 'invalid handshake prefix');
  requireThat(wire[3] === VERSION, 'invalid handshake version');
  const kind = wire[2];
  const fields = wire.subarray(4);
  validateFields(kind, fields);
  if (expectedKind !== undefined) requireThat(kind === expectedKind, 'unexpected handshake kind');
  return { kind, fields };
}

export const errorItem = (reason) => handshake(0x7f, Buffer.from([reason]));

export function pairKeys(K, request, offer, start, challenge) {
  [request, offer, start, challenge].forEach((wire, i) => parseHandshake(wire, 0x10 + i));
  const transcript = hash(ascii('touchdeck/net-v2/pair\0'), request, offer, start, challenge);
  const prk = extract(transcript, fixed(K, 64, 'SRP key'));
  const client = derive(prk, 'pair/client');
  const deck = derive(prk, 'pair/deck');
  const wrap = derive(prk, 'pair/wrap');
  const commit = derive(prk, 'pair/commit');
  const clientProof = mac(client, transcript);
  const deckProof = mac(deck, transcript, clientProof);
  return { transcript, prk, client, deck, wrap, commit, clientProof, deckProof };
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

// X25519 from RFC 7748 through Node's built-in key objects. The DER prefixes
// wrap a raw 32-byte key in PKCS #8 and SPKI for the X25519 OID 1.3.101.110.
const PKCS8 = Buffer.from('302e020100300506032b656e04220420', 'hex');
const SPKI = Buffer.from('302a300506032b656e032100', 'hex');
const privateKey = (secret) => createPrivateKey({ key: concat(PKCS8, fixed(secret, 32, 'X25519 secret')), format: 'der', type: 'pkcs8' });
const publicKey = (share) => createPublicKey({ key: concat(SPKI, fixed(share, 32, 'X25519 share')), format: 'der', type: 'spki' });

export function share(secret) {
  return createPublicKey(privateKey(secret)).export({ format: 'der', type: 'spki' }).subarray(-32);
}

export function x25519(secret, peerShare) {
  let Z;
  try {
    Z = diffieHellman({ privateKey: privateKey(secret), publicKey: publicKey(peerShare) });
  } catch {
    // OpenSSL refuses an all-zero result itself; treat it like the explicit check below.
    throw new Error('non-contributory X25519 share');
  }
  requireThat(Z.length === 32 && Z.some((byte) => byte !== 0), 'non-contributory X25519 share');
  return Z;
}

export function sessionKeys({ ltk, deviceId, companionId, clientShare, deviceShare, Z }) {
  const transcript = hash(ascii('touchdeck/net-v2/session\0'), fixed(deviceId, 6),
    fixed(companionId, 16), fixed(clientShare, 32), fixed(deviceShare, 32));
  const prk = extract(transcript, concat(fixed(ltk, 32, 'LTK'), fixed(Z, 32, 'Z')));
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

export function recordParameters(direction, counter, length) {
  requireThat(direction === 0 || direction === 1, 'invalid direction');
  requireThat(typeof counter === 'bigint' && counter >= 0n && counter < LIMIT, 'counter exhausted');
  requireThat(length >= MIN_RECORD_BODY && length <= MAX_RECORD_BODY, 'invalid record length');
  return {
    nonce: concat(Buffer.alloc(4), u64(counter)),
    aad: concat(ascii('TDN2'), Buffer.from([direction]), u64(counter), u16(length)),
  };
}

class Direction {
  constructor(key, direction) {
    this.key = Buffer.from(fixed(key, 32));
    this.direction = direction;
    this.counter = 0n;
    this.closed = false;
  }

  guard(action) {
    requireThat(!this.closed, 'direction closed');
    try {
      return action();
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

export class RecordSender extends Direction {
  seal(body) {
    return this.guard(() => {
      validateFrame(body);
      const length = body.length + 16;
      const { nonce, aad } = recordParameters(this.direction, this.counter, length);
      const wire = concat(u16(length), seal(this.key, nonce, body, aad));
      this.counter++;
      return wire;
    });
  }
}

export class RecordReceiver extends Direction {
  open(wire) {
    return this.guard(() => {
      requireThat(wire.length >= MIN_RECORD_BODY + 2 && wire.length <= MAX_ITEM, 'invalid record size');
      const length = wire.readUInt16BE();
      requireThat(length === wire.length - 2, 'invalid record prefix');
      const { nonce, aad } = recordParameters(this.direction, this.counter, length);
      const body = open(this.key, nonce, wire.subarray(2), aad);
      validateFrame(body);
      this.counter++;
      return body;
    });
  }
}

// BLE: one item per run of fragments. Every fragment is attMtu - 3 bytes except
// the item's last. attMtu is the negotiated value when the sender knows it, else 23.
export function fragment(item, attMtu = ATT_MTU_MIN) {
  requireThat(Number.isInteger(attMtu) && attMtu >= ATT_MTU_MIN && attMtu <= ATT_MTU_MAX, 'invalid ATT_MTU');
  requireThat(item.length >= 3 && item.length <= MAX_ITEM && item.readUInt16BE() === item.length - 2, 'invalid item');
  const size = attMtu - 3;
  const out = [];
  for (let offset = 0; offset < item.length; offset += size) out.push(item.subarray(offset, offset + size));
  return out;
}

export const PHASE_LIMITS = {
  handshake: [MIN_HANDSHAKE_BODY, MAX_HANDSHAKE_BODY],
  record: [MIN_RECORD_BODY, MAX_RECORD_BODY],
};

// Reassembles BLE fragments into items. The phase sets the allowed body length;
// the caller switches to 'record' after session.ready. The sender's F is learned
// from the first fragment that isn't an item's last, then checked.
export class Reassembler {
  constructor(maxFragment = ATT_MTU_MAX - 3) {
    this.maxFragment = maxFragment;
    this.phase = 'handshake';
    this.f = null;
    this.pending = null;
    this.closed = false;
  }

  records() {
    requireThat(this.pending === null, 'phase change inside an item');
    this.phase = 'record';
  }

  push(chunk) {
    requireThat(!this.closed, 'reassembler closed');
    try {
      requireThat(chunk.length >= 1 && chunk.length <= this.maxFragment, 'invalid fragment size');
      if (this.pending === null) {
        requireThat(chunk.length >= 2, 'fragment splits a length prefix');
        const length = chunk.readUInt16BE();
        const [min, max] = PHASE_LIMITS[this.phase];
        requireThat(length >= min && length <= max, 'invalid item length');
        this.pending = { need: length + 2, have: 0, parts: [] };
      }
      const { pending } = this;
      requireThat(pending.have + chunk.length <= pending.need, 'fragment crosses an item boundary');
      const last = pending.have + chunk.length === pending.need;
      if (last) {
        requireThat(this.f === null || chunk.length <= this.f, 'last fragment longer than F');
      } else {
        requireThat(chunk.length >= 20, 'middle fragment shorter than 20');
        if (this.f === null) this.f = chunk.length;
        requireThat(chunk.length === this.f, 'middle fragment differs from F');
      }
      pending.parts.push(Buffer.from(chunk));
      pending.have += chunk.length;
      if (!last) return null;
      this.pending = null;
      return concat(...pending.parts);
    } catch (error) {
      this.closed = true;
      throw error;
    }
  }
}

// Credit on the flow characteristic: a u64be cumulative limit of rx bytes.
export const creditValue = (limit) => u64(BigInt(limit));
