import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { hkdfSync } from 'node:crypto';
import {
  N, N_BYTES, LIMIT, ascii, bytes, concat, hash, hash512, minimum, pad, integer,
  derive, extract, identity, clientPublic, clientSecret, serverSecret, serverPublic,
  handshake, parseHandshake, pairKeys, wrapKey, acceptKey, commitProof, storedProof,
  sessionKeys, frame, validateFrame, crc16, seal, recordParameters, u16,
  RecordSender, RecordReceiver, PairingWindow,
} from './protocol.mjs';
import { inputs, pairing, session, buildVectors, readVectors } from './vectors.mjs';
import './socket-tests.mjs';

const fixture = readVectors();
const pair = pairing();
const keys = session();
const mutate = (buffer, offset = buffer.length - 1) => {
  const copy = Buffer.from(buffer);
  copy[offset] ^= 1;
  return copy;
};

test('reproduce every committed vector without rewriting it', () => {
  assert.deepEqual(buildVectors(), fixture);
  assert.equal(N_BYTES.length, 384);
  assert.equal(N.toString(2).length, 3072);
  assert.equal(crc16(ascii('123456789')), 0x29b1);
});

test('independent client and server SRP computations agree', () => {
  assert.equal(pair.client.S, pair.server.S);
  assert.deepEqual(pair.client.K, pair.server.K);
  assert.deepEqual(hash512(minimum(pair.client.S)), pair.client.K);
  assert.equal(pair.I.toString(), 'touchdeck:020000123456:101112131415161718191a1b1c1d1e1f');
});

test('HKDF labels match the built-in HKDF implementation', () => {
  for (const label of ['pair/client', 'pair/deck', 'pair/wrap', 'pair/commit']) {
    const expected = Buffer.from(hkdfSync('sha256', pair.client.K, pair.keys.transcript,
      ascii(`touchdeck/net-v1/${label}`), 32));
    assert.deepEqual(derive(extract(pair.keys.transcript, pair.client.K), label), expected);
  }
});

test('a leading-zero A and salt preserve their fixed widths', () => {
  const edge = fixture.leading_zero_case;
  const I = identity(inputs.deckId, inputs.appId);
  const a = bytes(edge.a);
  const salt = bytes(edge.salt);
  const client = clientSecret({ I, code: inputs.code, salt, a, B: bytes(edge.B) });
  const server = serverSecret({ I, code: inputs.code, salt, b: inputs.b, A: bytes(edge.A) });
  assert.equal(client.A.length, 384);
  assert.equal(integer(client.A), 5n);
  assert.equal(client.S, server.S);
  assert.equal(minimum(client.u).toString('hex'), edge.u);
  assert.notEqual(edge.u, edge.unpadded_u_is_wrong);
});

test('reject malicious public values before a proof is available', () => {
  const common = { I: pair.I, code: inputs.code, salt: inputs.salt };
  for (const value of [Buffer.alloc(384), pad(N), pad(N + 1n), Buffer.alloc(383), Buffer.alloc(385)]) {
    assert.throws(() => clientSecret({ ...common, a: inputs.a, B: value }));
    assert.throws(() => serverSecret({ ...common, b: inputs.b, A: value }));
  }
  assert.throws(() => clientPublic(Buffer.alloc(32)), /zero private/);
  assert.throws(() => serverPublic({ ...common, b: Buffer.alloc(32) }), /zero private/);
});

test('a wrong code and an impersonator with one guessed code cannot confirm the key', () => {
  const wrong = clientSecret({ I: pair.I, code: '123457', salt: inputs.salt, a: inputs.a, B: pair.server.B });
  const wrongKeys = pairKeys(wrong.K, pair.request, pair.offer, pair.start, pair.challenge);
  assert.notDeepEqual(wrongKeys.clientProof, pair.keys.clientProof);
  assert.throws(() => acceptKey(wrongKeys, pair.accept), /bad deck proof/);
  const fake = serverSecret({ I: pair.I, code: '654321', salt: inputs.salt, b: inputs.b, A: pair.client.A });
  const fakeChallenge = handshake(0x13, inputs.salt, fake.B, inputs.deckNonce);
  const honestClient = clientSecret({ I: pair.I, code: inputs.code, salt: inputs.salt, a: inputs.a, B: fake.B });
  const clientKeys = pairKeys(honestClient.K, pair.request, pair.offer, pair.start, fakeChallenge);
  const fakeKeys = pairKeys(fake.K, pair.request, pair.offer, pair.start, fakeChallenge);
  const fakeAccept = handshake(0x15, fakeKeys.deckProof, wrapKey(fakeKeys, inputs.ltk));
  assert.throws(() => acceptKey(clientKeys, fakeAccept), /bad deck proof/);
});

test('the transcript binds identity, app name, window, salt, public values, and nonces', () => {
  const wires = [pair.request, pair.offer, pair.start, pair.challenge];
  for (const [index, offset] of [[0, 4], [0, 21], [1, 4], [1, 10], [2, 4], [2, 388], [3, 4], [3, 20], [3, 404]]) {
    const changed = wires.map((wire, i) => i === index ? mutate(wire, offset) : wire);
    const other = pairKeys(pair.client.K, ...changed);
    assert.notDeepEqual(other.clientProof, pair.keys.clientProof);
    assert.throws(() => acceptKey(other, pair.accept), /bad deck proof/);
  }
});

test('pairing verifies the deck before opening the LTK envelope', () => {
  assert.deepEqual(acceptKey(pair.keys, pair.accept), inputs.ltk);
  assert.throws(() => acceptKey(pair.keys, mutate(pair.accept, 4)), /bad deck proof/);
  assert.throws(() => acceptKey(pair.keys, mutate(pair.accept)));
  assert.notDeepEqual(commitProof(pair.keys, inputs.ltk), storedProof(pair.keys, inputs.ltk));
  assert.notDeepEqual(commitProof(pair.keys, mutate(inputs.ltk)), commitProof(pair.keys, inputs.ltk));
});

test('each session identity, nonce, and LTK affects every proof and traffic key', () => {
  const base = { ...inputs, clientNonce: inputs.sessionClientNonce, deckNonce: inputs.sessionDeckNonce };
  for (const field of ['deckId', 'appId', 'clientNonce', 'deckNonce', 'ltk']) {
    const other = sessionKeys({ ...base, [field]: mutate(base[field]) });
    for (const output of ['deckProof', 'clientProof', 'readyProof', 'c2d', 'd2c']) {
      assert.notDeepEqual(other[output], keys[output]);
    }
  }
  assert.notDeepEqual(keys.deckProof, keys.clientProof);
  assert.notDeepEqual(keys.clientProof, keys.readyProof);
  assert.notDeepEqual(keys.c2d, keys.d2c);
});

test('open two deterministic records each way', () => {
  const receivers = { c2d: new RecordReceiver(keys.c2d, 0), d2c: new RecordReceiver(keys.d2c, 1) };
  for (const record of fixture.records) {
    assert.deepEqual(receivers[record.direction].open(bytes(record.wire)), bytes(record.plaintext));
  }
  assert.equal(receivers.c2d.counter, 2n);
  assert.equal(receivers.d2c.counter, 2n);
});

test('tamper, reflection, wrong key, replay, and reorder permanently close the receiver', () => {
  const first = bytes(fixture.records[0].wire);
  const second = bytes(fixture.records[1].wire);
  const cases = [
    [new RecordReceiver(keys.c2d, 0), [bytes(fixture.tampered_record.wire)]],
    [new RecordReceiver(keys.c2d, 0), [mutate(first, 2)]],
    [new RecordReceiver(keys.c2d, 1), [first]],
    [new RecordReceiver(keys.d2c, 0), [first]],
    [new RecordReceiver(keys.c2d, 0), [first, first]],
    [new RecordReceiver(keys.c2d, 0), [second]],
  ];
  for (const [receiver, records] of cases) {
    for (const record of records.slice(0, -1)) receiver.open(record);
    assert.throws(() => receiver.open(records.at(-1)));
    assert.equal(receiver.closed, true);
    assert.deepEqual(receiver.key, Buffer.alloc(32));
    assert.throws(() => receiver.open(first), /closed/);
  }
});

test('malformed length prefixes and truncation close the receiver', () => {
  const first = bytes(fixture.records[0].wire);
  for (const invalid of [Buffer.alloc(0), first.subarray(0, -1), concat(first, Buffer.from([0])),
    concat(u16(65535), first.subarray(2)), concat(u16(0), first.subarray(2))]) {
    const receiver = new RecordReceiver(keys.c2d, 0);
    assert.throws(() => receiver.open(invalid));
    assert.equal(receiver.closed, true);
    assert.equal(receiver.counter, 0n);
  }
});

test('a valid AEAD tag does not excuse an invalid link-v1 body', () => {
  const good = bytes(fixture.records[0].plaintext);
  for (const offset of [0, 1, 3, 4, good.length - 1]) {
    const body = mutate(good, offset);
    const { nonce, aad } = recordParameters(0, 0n, body.length + 16);
    const wire = concat(u16(body.length + 16), seal(keys.c2d, nonce, body, aad));
    const receiver = new RecordReceiver(keys.c2d, 0);
    assert.throws(() => receiver.open(wire));
    assert.equal(receiver.closed, true);
  }
});

test('preserve every valid pre-existing golden link-v1 frame unchanged', () => {
  const golden = JSON.parse(readFileSync(new URL('../golden-frames.json', import.meta.url), 'utf8'));
  let count = 0;
  for (const entry of golden.frames.filter((entry) => entry.valid)) {
    const escaped = bytes(entry.hex).subarray(1, -1);
    const decoded = [];
    for (let i = 0; i < escaped.length; i++) {
      decoded.push(escaped[i] === 0xdb ? (escaped[++i] === 0xdc ? 0xc0 : 0xdb) : escaped[i]);
    }
    const body = Buffer.from(decoded);
    validateFrame(body);
    const sender = new RecordSender(keys.c2d, 0);
    const receiver = new RecordReceiver(keys.c2d, 0);
    assert.deepEqual(receiver.open(sender.seal(body)), body, entry.name);
    count++;
  }
  assert.ok(count >= 10);
});

test('maximum payload and chunk retransmission use distinct record counters', () => {
  const payload = Buffer.alloc(1024, 0xc0);
  const body = frame(payload, { type: 2 });
  const sender = new RecordSender(keys.c2d, 0);
  const receiver = new RecordReceiver(keys.c2d, 0);
  const first = sender.seal(body);
  const retry = sender.seal(body);
  assert.equal(first.length, 1050);
  assert.notDeepEqual(first, retry);
  assert.deepEqual(receiver.open(first), body);
  assert.deepEqual(receiver.open(retry), body);
});

test('record counters cannot wrap or exceed the per-key bound', () => {
  const body = frame({ m: 'ping' });
  const sender = new RecordSender(keys.c2d, 0);
  const receiver = new RecordReceiver(keys.c2d, 0);
  sender.counter = receiver.counter = LIMIT - 1n;
  assert.deepEqual(receiver.open(sender.seal(body)), body);
  assert.throws(() => sender.seal(body), /counter exhausted/);
  assert.throws(() => receiver.open(bytes(fixture.records[0].wire)), /counter exhausted/);
  assert.equal(sender.closed, true);
  assert.equal(receiver.closed, true);
});

test('handshake parsing rejects invalid kinds, version, UTF-8, names, and trailing bytes', () => {
  assert.equal(parseHandshake(handshake(0x7f, Buffer.from([1]))).kind, 0x7f);
  assert.equal(handshake(0x7f, Buffer.from([1])).toString('hex'), '00037f0101');
  assert.throws(() => parseHandshake(pair.offer, 0x12));
  for (const invalid of [mutate(pair.request, 3), mutate(pair.request, 0),
    concat(pair.request, Buffer.from([0])), Buffer.from([0, 2, 0x55, 1])]) {
    assert.throws(() => parseHandshake(invalid));
  }
  for (const name of [Buffer.alloc(0), Buffer.alloc(65, 65), Buffer.from([0xff]), Buffer.from([0])]) {
    assert.throws(() => handshake(0x10, inputs.appId, Buffer.from([name.length]), name));
  }
  assert.throws(() => handshake(0x10, inputs.appId, Buffer.from([2]), ascii('a')));
});

test('three wrong proofs end one window and a new window starts with no failures', () => {
  const window = new PairingWindow(100);
  for (let attempt = 0; attempt < 3; attempt++) {
    window.start(200 + attempt * 1000);
    window.proof(false, 300 + attempt * 1000);
  }
  assert.equal(window.failures, 3);
  assert.equal(window.state, 'closed');
  assert.throws(() => window.start(4000));
  assert.equal(new PairingWindow(4000).failures, 0);
});

test('human entry waits up to 120 seconds but each active attempt has only 10 seconds', () => {
  const window = new PairingWindow(0);
  window.start(60000);
  window.proof(true, 69000);
  window.commit(69999);
  assert.equal(window.state, 'closed');
  const timeout = new PairingWindow(0);
  timeout.start(500);
  assert.throws(() => timeout.proof(true, 10500));
  const expired = new PairingWindow(0);
  expired.start(119000);
  assert.throws(() => expired.proof(true, 120000));
  const lateCommit = new PairingWindow(0);
  lateCommit.start(100);
  lateCommit.proof(true, 101);
  assert.throws(() => lateCommit.commit(10100));
});

test('cancellation and out-of-order pairing transitions cannot commit', () => {
  for (const transition of ['proof', 'commit']) {
    const invalid = new PairingWindow(0);
    assert.throws(() => transition === 'proof' ? invalid.proof(true, 10) : invalid.commit(10));
    assert.equal(invalid.state, 'closed');
  }
  const duplicate = new PairingWindow(0);
  duplicate.start(20);
  assert.throws(() => duplicate.start(21));
  assert.equal(duplicate.state, 'closed');
  const window = new PairingWindow(0);
  window.start(20);
  window.cancel();
  assert.throws(() => window.proof(true, 22));
  assert.throws(() => window.commit(23));
});
