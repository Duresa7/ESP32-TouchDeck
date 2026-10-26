import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import {
  N_BYTES, G, ascii, bytes, concat, hash512, minimum, pad, multiplier,
  identity, handshake, u32, clientPublic, clientSecret, serverSecret, pairKeys,
  wrapKey, commitProof, storedProof, sessionKeys, frame, RecordSender, recordParameters,
} from './protocol.mjs';

const ramp = (start, length) => Buffer.from(Array.from({ length }, (_, i) => (start + i) & 255));
const hexObject = (values) => Object.fromEntries(Object.entries(values).map(([key, value]) =>
  [key, typeof value === 'bigint' ? minimum(value).toString('hex') : value.toString('hex')]));

export const inputs = {
  deckId: bytes('020000123456'), appId: ramp(0x10, 16), appName: 'Vector workstation',
  code: '123456', salt: ramp(0x30, 16), a: ramp(0x50, 32), b: ramp(0x70, 32),
  windowId: ramp(0x90, 16), clientNonce: ramp(0xa0, 32), deckNonce: ramp(0xc0, 32),
  ltk: ramp(0xe0, 32), sessionClientNonce: ramp(0x20, 32), sessionDeckNonce: ramp(0x40, 32),
};

export function pairing(fixture = inputs) {
  const I = identity(fixture.deckId, fixture.appId);
  const name = Buffer.from(fixture.appName);
  const request = handshake(0x10, fixture.appId, Buffer.from([name.length]), name);
  const offer = handshake(0x11, fixture.deckId, fixture.windowId, u32(120000));
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
  return sessionKeys({ ...fixture, clientNonce: fixture.sessionClientNonce, deckNonce: fixture.sessionDeckNonce });
}

export function buildVectors() {
  const pair = pairing();
  const keys = session();
  const c2d = new RecordSender(keys.c2d, 0);
  const d2c = new RecordSender(keys.d2c, 1);
  const bodies = [
    frame({ m: 'hello' }),
    frame({ m: 'ping', n: 0 }, { seq: 2 }),
    frame({ m: 'hello', ok: true, product: 'touchdeck', proto: 1 }, { flags: 1 }),
    frame({ m: 'ping', ok: true, n: 0, t: 1234 }, { seq: 2, flags: 1 }),
  ];
  const records = bodies.map((body, index) => {
    const direction = index < 2 ? 0 : 1;
    const counter = BigInt(index % 2);
    const { nonce, aad } = recordParameters(direction, counter, body.length + 16);
    return { direction: direction === 0 ? 'c2d' : 'd2c', counter: Number(counter),
      plaintext: body.toString('hex'), nonce: nonce.toString('hex'), aad: aad.toString('hex'),
      wire: (direction === 0 ? c2d : d2c).seal(body).toString('hex') };
  });
  const tampered = bytes(records[0].wire);
  tampered[tampered.length - 1] ^= 1;
  const edge = pairing({ ...inputs, a: concat(Buffer.alloc(31), Buffer.from([1])), salt: Buffer.alloc(16) });
  return {
    protocol: 'touchdeck/net-v1', fixture_notice: 'Public synthetic secrets. Never use on a real deck.',
    suite: 'SRP-6a-3072-SHA512/HKDF-SHA256/AES-256-GCM',
    group: { N: N_BYTES.toString('hex'), g: G.toString(), k: minimum(multiplier()).toString('hex') },
    inputs: { ...hexObject(Object.fromEntries(Object.entries(inputs).filter(([, v]) => Buffer.isBuffer(v)))),
      code: inputs.code, appName: inputs.appName },
    pake: { identity: pair.I.toString('ascii'), ...hexObject({ A: pair.client.A, B: pair.server.B,
      x: pair.client.x, v: pair.server.v, u: pair.client.u, S: minimum(pair.client.S),
      K_srp: pair.client.K }), server_S: minimum(pair.server.S).toString('hex') },
    pairing: { ...hexObject(pair.keys), wrapped_key: pair.wrapped.toString('hex'),
      wire: hexObject(Object.fromEntries(['request', 'offer', 'start', 'challenge', 'proof', 'accept', 'commit', 'stored']
        .map((name) => [name, pair[name]]))) },
    session: { ...hexObject(keys), wire: hexObject({
      open: handshake(0x20, inputs.deckId, inputs.appId, inputs.sessionClientNonce),
      challenge: handshake(0x21, inputs.sessionDeckNonce, keys.deckProof),
      proof: handshake(0x22, keys.clientProof), ready: handshake(0x23, keys.readyProof),
    }) },
    records,
    tampered_record: { direction: 'c2d', counter: 0, wire: tampered.toString('hex'), expected: 'close' },
    leading_zero_case: { a: concat(Buffer.alloc(31), Buffer.from([1])).toString('hex'),
      salt: Buffer.alloc(16).toString('hex'), ...hexObject({ A: edge.client.A, B: edge.server.B,
        u: edge.client.u, S: minimum(edge.client.S), K_srp: edge.client.K }),
      unpadded_u_is_wrong: hash512(minimum(5n), edge.server.B).toString('hex'),
      padded_g: pad(G).toString('hex') },
  };
}

export const vectorPath = new URL('../net-v1-vectors.json', import.meta.url);
export const readVectors = () => JSON.parse(readFileSync(vectorPath, 'utf8'));

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] !== '--write') throw new Error('Use --write to regenerate public test fixtures');
  writeFileSync(vectorPath, `${JSON.stringify(buildVectors(), null, 2)}\n`);
  process.stdout.write('Wrote net-v1-vectors.json with public synthetic test values.\n');
}
