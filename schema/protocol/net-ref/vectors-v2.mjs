// Regenerates schema/protocol/net-v2-vectors.json from public, deterministic
// inputs. Every key and secret here is a synthetic test value. Never use one on
// a real Device. Run from the repository root:
//   node schema/protocol/net-ref/vectors-v2.mjs --write
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import {
  bytes, concat, minimum, identity, clientPublic, clientSecret, serverSecret, u16, u32, frame,
} from './protocol.mjs';
import {
  gatt, handshake, errorItem, pairKeys, wrapKey, commitProof, storedProof,
  share, x25519, sessionKeys, recordParameters, RecordSender, fragment, creditValue,
  MIN_INITIAL_CREDIT, FORMAT_RANGES, VERSION,
} from './net-v2.mjs';

const ramp = (start, length) => Buffer.from(Array.from({ length }, (_, i) => (start + i) & 255));
const hex = (value) => (typeof value === 'bigint' ? minimum(value) : value).toString('hex');
const hexObject = (values) => Object.fromEntries(Object.entries(values).map(([key, value]) => [key, hex(value)]));

export const inputs = {
  deviceId: bytes('020000000001'), companionId: ramp(0x10, 16), companionName: 'Vector workstation',
  code: '123456', salt: ramp(0x30, 16), a: ramp(0x50, 32), b: ramp(0x70, 32),
  windowId: ramp(0x90, 16), clientNonce: ramp(0xa0, 32), deckNonce: ramp(0xc0, 32),
  ltk: ramp(0xe0, 32), clientEphemeral: ramp(0x01, 32), deviceEphemeral: ramp(0x21, 32),
};

export const goldenFrames = JSON.parse(readFileSync(new URL('../golden-frames-v2.json', import.meta.url), 'utf8')).frames;

// The link body of a golden-frames-v2 fixture, rebuilt from its decoded fields.
export function goldenBody(name, override) {
  const fixture = goldenFrames.find((entry) => entry.name === name);
  if (!fixture) throw new Error(`missing golden frame ${name}`);
  const { type, seq, flags, payload_hex: payloadHex } = fixture.decoded;
  const payload = override ? Buffer.from(JSON.stringify({ ...fixture.message, ...override })) : bytes(payloadHex);
  return frame(payload, { type, seq, flags });
}

export function pairing(fixture = inputs) {
  const I = identity(fixture.deviceId, fixture.companionId);
  const name = Buffer.from(fixture.companionName);
  const request = handshake(0x10, fixture.companionId, Buffer.from([name.length]), name);
  const offer = handshake(0x11, fixture.deviceId, fixture.windowId, u32(120000));
  const A = clientPublic(fixture.a);
  const start = handshake(0x12, A, fixture.clientNonce);
  const server = serverSecret({ I, code: fixture.code, salt: fixture.salt, b: fixture.b, A });
  const client = clientSecret({ I, code: fixture.code, salt: fixture.salt, a: fixture.a, B: server.B });
  const challenge = handshake(0x13, fixture.salt, server.B, fixture.deckNonce);
  const keys = pairKeys(client.K, request, offer, start, challenge);
  const wrapped = wrapKey(keys, fixture.ltk);
  const proof = handshake(0x14, keys.clientProof);
  const accept = handshake(0x15, keys.deckProof, wrapped);
  const commit = handshake(0x16, commitProof(keys, fixture.ltk));
  const stored = handshake(0x17, storedProof(keys, fixture.ltk));
  return { I, client, server, keys, wrapped, request, offer, start, challenge, proof, accept, commit, stored };
}

export function session(fixture = inputs) {
  const clientShare = share(fixture.clientEphemeral);
  const deviceShare = share(fixture.deviceEphemeral);
  const Z = x25519(fixture.clientEphemeral, deviceShare);
  const keys = sessionKeys({ ltk: fixture.ltk, deviceId: fixture.deviceId, companionId: fixture.companionId,
    clientShare, deviceShare, Z });
  const wire = {
    open: handshake(0x20, fixture.deviceId, fixture.companionId, clientShare),
    challenge: handshake(0x21, deviceShare, keys.deckProof),
    proof: handshake(0x22, keys.clientProof),
    ready: handshake(0x23, keys.readyProof),
  };
  return { clientShare, deviceShare, Z, keys, wire };
}

export function recordBodies() {
  return [
    { name: 'hello-discovery-request', direction: 0, body: goldenBody('hello-discovery-request') },
    { name: 'hello-discovery-response', direction: 1, body: goldenBody('hello-discovery-response') },
    { name: 'hello-selected-request', direction: 0, body: goldenBody('hello-selected-request') },
    { name: 'hello-selected-response', direction: 1,
      body: goldenBody('hello-selected-response', { id: inputs.deviceId.toString('hex') }) },
  ];
}

// A pair.request built without the name check, so rejected names can be encoded.
export function rawRequest(name, companionId = inputs.companionId) {
  const encoded = Buffer.from(name, 'utf8');
  return concat(u16(encoded.length + 19), Buffer.from([0x10, VERSION]), companionId,
    Buffer.from([encoded.length]), encoded);
}

const hex4 = (codePoint) => codePoint.toString(16).padStart(4, '0');
const codePointLabel = (codePoint) => `U+${hex4(codePoint).toUpperCase()}`;
const rangeLabel = ([first, last]) => (first === last ? hex4(first) : `${hex4(first)}-${hex4(last)}`);

// Names every implementation MUST accept, and names with one control or format
// character that it MUST refuse by closing the carrier connection.
export const acceptedNames = [
  ['ascii', 'Desk PC'],
  ['accented', 'Bureau d\u2019\u00e9t\u00e9'],
  ['right-to-left letters', '\u05de\u05d7\u05e9\u05d1'],
  ['emoji with variation selector', 'Desk \u{1f5a5}\ufe0f'],
  ['unassigned neighbour U+2065', 'Desk\u2065PC'],
];
export const rejectedCodePoints = [
  0x0000, 0x001b, 0x007f, 0x0085, 0x00ad, 0x061c, 0x115f, 0x1160, 0x180e, 0x200b, 0x200c,
  0x200d, 0x200e, 0x200f, 0x202a, 0x202b, 0x202c, 0x202d, 0x202e, 0x2060, 0x2066, 0x2067,
  0x2068, 0x2069, 0x3164, 0xfeff, 0xffa0, 0xe0001, 0xe0041,
];

export function companionNames() {
  return {
    format_ranges: FORMAT_RANGES.map(rangeLabel),
    accepted: acceptedNames.map(([label, name]) => ({ label, request: hex(rawRequest(name)) })),
    rejected: [
      ...rejectedCodePoints.map((codePoint) => ({ label: codePointLabel(codePoint),
        request: hex(rawRequest(`Desk${String.fromCodePoint(codePoint)}PC`)), expected: 'close' })),
      { label: 'leading U+FEFF', request: hex(rawRequest('\ufeffDesk PC')), expected: 'close' },
    ],
  };
}

export function buildVectors() {
  const pair = pairing();
  const sess = session();
  const senders = [new RecordSender(sess.keys.c2d, 0), new RecordSender(sess.keys.d2c, 1)];
  const counters = [0n, 0n];
  const records = recordBodies().map(({ name, direction, body }) => {
    const counter = counters[direction]++;
    const { nonce, aad } = recordParameters(direction, counter, body.length + 16);
    const wire = senders[direction].seal(body);
    const entry = { name, direction: direction === 0 ? 'c2d' : 'd2c', counter: Number(counter),
      plaintext: hex(body), nonce: hex(nonce), aad: hex(aad), wire: hex(wire) };
    if (name.startsWith('hello-selected')) {
      entry.ble_fragments = {
        att_mtu_23: fragment(wire, 23).map(hex),
        att_mtu_247: fragment(wire, 247).map(hex),
      };
    }
    return entry;
  });
  const tampered = bytes(records[0].wire);
  tampered[tampered.length - 1] ^= 1;
  return {
    protocol: 'touchdeck/net-v2',
    fixture_notice: 'Public synthetic test values only. Never use these keys, codes or identities on a real Device.',
    suite: 'SRP-6a-3072-SHA512/X25519/HKDF-SHA256/AES-256-GCM',
    inputs: { ...hexObject(Object.fromEntries(Object.entries(inputs).filter(([, v]) => Buffer.isBuffer(v)))),
      code: inputs.code, companionName: inputs.companionName },
    pake: { identity: pair.I.toString('ascii'), ...hexObject({ A: pair.client.A, B: pair.server.B,
      u: pair.client.u, S: pair.client.S, K_srp: pair.client.K }) },
    pairing: { ...hexObject(pair.keys), wrapped_key: hex(pair.wrapped),
      wire: hexObject(Object.fromEntries(['request', 'offer', 'start', 'challenge', 'proof', 'accept', 'commit', 'stored']
        .map((name) => [name, pair[name]]))) },
    session: { ...hexObject({ client_share: sess.clientShare, device_share: sess.deviceShare, Z: sess.Z }),
      ...hexObject(sess.keys), wire: hexObject(sess.wire) },
    records,
    tampered_record: { direction: 'c2d', counter: 0, wire: hex(tampered), expected: 'close' },
    non_contributory_share: { device_share: hex(Buffer.alloc(32)), expected: 'close' },
    companion_names: companionNames(),
    errors: { busy: hex(errorItem(1)), closed: hex(errorItem(9)) },
    ble: { ...gatt, initial_credit: MIN_INITIAL_CREDIT * 2, initial_credit_value: hex(creditValue(MIN_INITIAL_CREDIT * 2)) },
  };
}

export const vectorPath = new URL('../net-v2-vectors.json', import.meta.url);
export const readVectors = () => JSON.parse(readFileSync(vectorPath, 'utf8'));

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] !== '--write') throw new Error('Use --write to regenerate public test fixtures');
  writeFileSync(vectorPath, `${JSON.stringify(buildVectors(), null, 2)}\n`);
  process.stdout.write('Wrote net-v2-vectors.json with public synthetic test values.\n');
}
