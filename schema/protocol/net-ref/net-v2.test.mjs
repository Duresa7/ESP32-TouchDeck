import test from 'node:test';
import assert from 'node:assert/strict';
import { hkdfSync } from 'node:crypto';
import { ascii, bytes, concat, hash512, minimum, extract, validateFrame } from './protocol.mjs';
import {
  derive, handshake, parseHandshake, acceptKey, share, x25519, sessionKeys,
  RecordSender, RecordReceiver, fragment, Reassembler, creditValue, gatt,
  MAX_ITEM, MIN_INITIAL_CREDIT,
} from './net-v2.mjs';
import { inputs, goldenFrames, pairing, session, buildVectors, readVectors } from './vectors-v2.mjs';

const fixture = readVectors();
const pair = pairing();
const sess = session();

function unslip(hex) {
  const wire = bytes(hex);
  const out = [];
  for (let i = 1; i < wire.length - 1; i++) {
    if (wire[i] === 0xdb) out.push(wire[++i] === 0xdc ? 0xc0 : 0xdb);
    else out.push(wire[i]);
  }
  return Buffer.from(out);
}

test('net-v2: reproduce every committed vector without rewriting it', () => {
  assert.deepEqual(buildVectors(), fixture);
  assert.match(fixture.fixture_notice, /synthetic/);
});

test('net-v2: X25519 matches RFC 7748 section 6.1', () => {
  const alice = bytes('77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a');
  const bob = bytes('5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb');
  assert.equal(share(alice).toString('hex'), '8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a');
  assert.equal(share(bob).toString('hex'), 'de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f');
  const shared = '4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742';
  assert.equal(x25519(alice, share(bob)).toString('hex'), shared);
  assert.equal(x25519(bob, share(alice)).toString('hex'), shared);
});

test('net-v2: both sides derive the same session keys', () => {
  const deviceZ = x25519(inputs.deviceEphemeral, sess.clientShare);
  assert.deepEqual(deviceZ, sess.Z);
  const device = sessionKeys({ ltk: inputs.ltk, deviceId: inputs.deviceId, companionId: inputs.companionId,
    clientShare: sess.clientShare, deviceShare: sess.deviceShare, Z: deviceZ });
  assert.deepEqual(device, sess.keys);
});

test('net-v2: HKDF labels match the built-in HKDF with LTK || Z as input keying material', () => {
  const ikm = concat(inputs.ltk, sess.Z);
  for (const label of ['session/deck', 'session/client', 'session/ready', 'session/c2d', 'session/d2c']) {
    const expected = Buffer.from(hkdfSync('sha256', ikm, sess.keys.transcript, ascii(`touchdeck/net-v2/${label}`), 32));
    assert.deepEqual(derive(extract(sess.keys.transcript, ikm), label), expected);
  }
  const pairExpected = Buffer.from(hkdfSync('sha256', pair.client.K, pair.keys.transcript, ascii('touchdeck/net-v2/pair/wrap'), 32));
  assert.deepEqual(pair.keys.wrap, pairExpected);
});

test('net-v2: SRP pairing agrees and delivers the LTK only after the Device proof', () => {
  assert.equal(pair.client.S, pair.server.S);
  assert.deepEqual(hash512(minimum(pair.client.S)), pair.client.K);
  assert.deepEqual(acceptKey(pair.keys, pair.accept), inputs.ltk);
  const forged = Buffer.from(pair.accept);
  forged[4] ^= 1;
  assert.throws(() => acceptKey(pair.keys, forged), /bad deck proof/);
});

test('net-v2: a wrong LTK or a changed share fails the Device proof', () => {
  const base = { ltk: inputs.ltk, deviceId: inputs.deviceId, companionId: inputs.companionId,
    clientShare: sess.clientShare, deviceShare: sess.deviceShare, Z: sess.Z };
  assert.notDeepEqual(sessionKeys({ ...base, ltk: Buffer.alloc(32, 7) }).deckProof, sess.keys.deckProof);
  const otherShare = share(Buffer.alloc(32, 9));
  assert.notDeepEqual(sessionKeys({ ...base, clientShare: otherShare }).deckProof, sess.keys.deckProof);
});

test('net-v2: non-contributory X25519 shares close the handshake', () => {
  assert.throws(() => x25519(inputs.clientEphemeral, bytes(fixture.non_contributory_share.device_share)), /non-contributory/);
  const orderOne = concat(Buffer.from([1]), Buffer.alloc(31));
  assert.throws(() => x25519(inputs.clientEphemeral, orderOne), /non-contributory/);
});

test('net-v2: handshake items carry version 2 and refuse net-v1 items', () => {
  for (const wire of Object.values(fixture.session.wire)) assert.equal(bytes(wire)[3], 2);
  assert.equal(fixture.errors.busy, '00037f0201');
  const v1 = Buffer.from(bytes(fixture.session.wire.proof));
  v1[3] = 1;
  assert.throws(() => parseHandshake(v1), /version/);
  assert.throws(() => handshake(0x20, Buffer.alloc(53)), /length/);
});

test('net-v2: records open each way and carry the golden link-v2 bodies', () => {
  const receivers = { c2d: new RecordReceiver(sess.keys.c2d, 0), d2c: new RecordReceiver(sess.keys.d2c, 1) };
  for (const record of fixture.records) {
    const body = receivers[record.direction].open(bytes(record.wire));
    assert.equal(body.toString('hex'), record.plaintext);
    const golden = goldenFrames.find((entry) => entry.name === record.name);
    if (record.name !== 'hello-selected-response') assert.deepEqual(body, unslip(golden.hex));
  }
  const selected = validateFrame(bytes(fixture.records[3].plaintext));
  const hello = JSON.parse(selected.payload.toString());
  assert.equal(hello.proto, 2);
  assert.equal(hello.id, fixture.inputs.deviceId);
});

test('net-v2: tampered, replayed and reordered records close the receiver', () => {
  const tampered = new RecordReceiver(sess.keys.c2d, 0);
  assert.throws(() => tampered.open(bytes(fixture.tampered_record.wire)));
  assert.throws(() => tampered.open(bytes(fixture.records[0].wire)), /closed/);

  const replay = new RecordReceiver(sess.keys.c2d, 0);
  replay.open(bytes(fixture.records[0].wire));
  assert.throws(() => replay.open(bytes(fixture.records[0].wire)));

  const reorder = new RecordReceiver(sess.keys.c2d, 0);
  assert.throws(() => reorder.open(bytes(fixture.records[2].wire)));

  const sender = new RecordSender(sess.keys.c2d, 0);
  assert.throws(() => new RecordReceiver(sess.keys.c2d, 1).open(sender.seal(bytes(fixture.records[0].plaintext))));
});

test('net-v2: BLE fragments are full size except the last and reassemble to the record', () => {
  for (const record of fixture.records.filter((entry) => entry.ble_fragments)) {
    for (const [key, mtu] of [['att_mtu_23', 23], ['att_mtu_247', 247]]) {
      const parts = record.ble_fragments[key].map(bytes);
      parts.slice(0, -1).forEach((part) => assert.equal(part.length, mtu - 3));
      assert.ok(parts.at(-1).length >= 1 && parts.at(-1).length <= mtu - 3);
      const reassembler = new Reassembler(mtu - 3);
      const items = parts.map((part) => reassembler.push(part)).filter(Boolean);
      assert.deepEqual(items, [bytes(record.wire)]);
    }
  }
  const handshakeParts = fragment(bytes(fixture.pairing.wire.start), 23);
  assert.equal(handshakeParts.length, Math.ceil(420 / 20));
});

test('net-v2: BLE reassembly refuses fragments that break item boundaries', () => {
  const wire = bytes(fixture.records[2].wire);
  const crossing = new Reassembler(64);
  crossing.push(wire.subarray(0, 20));
  assert.throws(() => crossing.push(concat(wire.subarray(20), Buffer.from([0]))), /boundary/);
  assert.throws(() => crossing.push(Buffer.from([0, 0])), /closed/);
  assert.throws(() => new Reassembler(20).push(Buffer.alloc(0)), /fragment size/);
  assert.throws(() => new Reassembler(20).push(Buffer.alloc(21)), /fragment size/);
  assert.throws(() => new Reassembler(20).push(Buffer.from([0])), /prefix/);
  assert.throws(() => new Reassembler(20).push(Buffer.from([0x04, 0x19])), /item length/);
});

test('net-v2: BLE service constants and initial credit', () => {
  assert.equal(new Set([gatt.service, gatt.rx, gatt.tx, gatt.flow]).size, 4);
  assert.equal(MAX_ITEM, 1050);
  assert.ok(fixture.ble.initial_credit >= MIN_INITIAL_CREDIT);
  assert.equal(creditValue(fixture.ble.initial_credit).toString('hex'), fixture.ble.initial_credit_value);
});
