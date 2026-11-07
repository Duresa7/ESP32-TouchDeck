// Runs every shared negative vector in net-v2-vectors.json through the
// reference. The Firmware and the Companion run the same vectors.
import test from 'node:test';
import assert from 'node:assert/strict';
import { bytes, LIMIT } from './protocol.mjs';
import {
  handshake, errorItem, pairKeys, acceptKey, verifyPairProof, share, x25519, rawX25519, canonicalShare,
  DeviceSession, CompanionSession, RecordSender, RecordReceiver, Reassembler, FlowCredit, fragment,
  P25519, MAX_ITEM,
} from './net-v2.mjs';
import { inputs, pairing, session, readVectors } from './vectors-v2.mjs';

const fixture = readVectors();
const negative = fixture.negative;
const pair = pairing();
const sess = session();

// 'accept', a handshake error reason such as 'auth', or 'close'.
function outcome(action) {
  try {
    action();
    return 'accept';
  } catch (error) {
    return error.reason ?? 'close';
  }
}

const device = () => new DeviceSession({ deviceId: inputs.deviceId,
  pairings: new Map([[inputs.companionId.toString('hex'), inputs.ltk]]), ephemeral: inputs.deviceEphemeral });
const companion = () => new CompanionSession({ deviceId: inputs.deviceId, companionId: inputs.companionId,
  ltk: inputs.ltk, ephemeral: inputs.clientEphemeral });

test('net-v2 negative: every shared vector has a test here', () => {
  assert.match(negative.notice, /synthetic/);
  assert.deepEqual(Object.keys(negative).sort(), [
    'counter_limit', 'credit_overrun', 'inputs', 'max_record', 'notice', 'oversize_items',
    'swapped_client_share', 'swapped_device_share', 'tampered_wrapped_key', 'unknown_companion_id',
    'write_before_credit', 'wrong_code', 'wrong_device_id', 'x25519',
  ]);
});

test('net-v2 negative: the reference handshake accepts the positive session vectors', () => {
  const d = device();
  const c = companion();
  assert.deepEqual(c.open(), bytes(fixture.session.wire.open));
  const challenge = d.open(c.open());
  assert.equal(challenge.toString('hex'), fixture.session.wire.challenge);
  const proof = c.challenge(challenge);
  assert.equal(proof.toString('hex'), fixture.session.wire.proof);
  const ready = d.proof(proof);
  assert.equal(ready.toString('hex'), fixture.session.wire.ready);
  assert.deepEqual(c.ready(ready), sess.keys);
});

test('net-v2 negative: a wrong pairing code gets auth', () => {
  const { proof, expected, error } = negative.wrong_code;
  const deviceKeys = pairKeys(pair.server.K, pair.request, pair.offer, pair.start, pair.challenge);
  assert.equal(outcome(() => verifyPairProof(deviceKeys, pair.proof)), 'accept');
  assert.equal(outcome(() => verifyPairProof(deviceKeys, bytes(proof))), expected);
  assert.equal(errorItem(5).toString('hex'), error);
});

test('net-v2 negative: a tampered wrapped_key never yields an LTK', () => {
  assert.deepEqual(acceptKey(pair.keys, pair.accept), inputs.ltk);
  for (const { accept, expected } of negative.tampered_wrapped_key) {
    assert.equal(outcome(() => acceptKey(pair.keys, bytes(accept))), expected);
  }
});

test('net-v2 negative: a swapped client share fails the client proof on the Device', () => {
  const { open, challenge, proof, expected } = negative.swapped_client_share;
  const d = device();
  assert.equal(d.open(bytes(open)).toString('hex'), challenge);
  assert.equal(outcome(() => d.proof(bytes(proof))), expected);
});

test('net-v2 negative: a swapped device share fails the device proof on the Companion', () => {
  const { challenge, expected } = negative.swapped_device_share;
  const c = companion();
  c.open();
  assert.equal(outcome(() => c.challenge(bytes(challenge))), expected);
  assert.equal(c.keys, null);
});

test('net-v2 negative: a wrong device_id or an unknown companion_id gets auth', () => {
  for (const name of ['wrong_device_id', 'unknown_companion_id']) {
    const { open, expected, error } = negative[name];
    const d = device();
    assert.equal(outcome(() => d.open(bytes(open))), expected, name);
    assert.equal(d.keys, null, name);
    assert.equal(errorItem(5).toString('hex'), error);
  }
});

test('net-v2 negative: the record counter stops at 2^32 even with a valid tag', () => {
  const v = negative.counter_limit;
  assert.equal(BigInt(v.last_counter), LIMIT - 1n);
  assert.equal(BigInt(v.beyond_counter), LIMIT);

  const sender = new RecordSender(sess.keys.c2d, 0);
  sender.counter = LIMIT - 1n;
  assert.equal(sender.seal(bytes(v.plaintext)).toString('hex'), v.last);
  assert.throws(() => sender.seal(bytes(v.plaintext)), /counter exhausted/);

  const receiver = new RecordReceiver(sess.keys.c2d, 0);
  receiver.counter = LIMIT - 1n;
  assert.equal(outcome(() => receiver.open(bytes(v.last))), v.last_expected);
  assert.equal(outcome(() => receiver.open(bytes(v.beyond))), v.beyond_expected);

  const fresh = new RecordReceiver(sess.keys.c2d, 0);
  fresh.counter = LIMIT;
  assert.equal(outcome(() => fresh.open(bytes(v.beyond))), 'close');
});

test('net-v2 negative: a maximum-size record opens, and oversize prefixes close', () => {
  const { wire, plaintext, expected } = negative.max_record;
  assert.equal(bytes(wire).length, MAX_ITEM);
  assert.equal(expected, 'accept');
  assert.equal(new RecordReceiver(sess.keys.c2d, 0).open(bytes(wire)).toString('hex'), plaintext);
  for (const mtu of [23, 247]) {
    const reassembler = new Reassembler(mtu - 3);
    reassembler.records();
    const items = fragment(bytes(wire), mtu).map((part) => reassembler.push(part)).filter(Boolean);
    assert.deepEqual(items, [bytes(wire)]);
  }
  for (const { phase, prefix, expected: result } of negative.oversize_items) {
    const reassembler = new Reassembler();
    if (phase === 'record') reassembler.records();
    assert.equal(outcome(() => reassembler.push(bytes(prefix))), result, `${phase} ${prefix}`);
  }
});

function runCredit({ initial_credit: initial, events }) {
  const credit = new FlowCredit(initial);
  let accepted = 0;
  const result = outcome(() => {
    for (const event of events) {
      if (event.enable) credit.enable(event.enable);
      for (let i = 0; i < (event.repeat ?? 1) && event.write; i++) {
        credit.write(event.write);
        accepted += event.write;
      }
    }
  });
  return { result, accepted };
}

test('net-v2 negative: a write before the carrier connection starts closes it', () => {
  const v = negative.write_before_credit;
  assert.equal(runCredit(v).result, v.expected);
  const started = new FlowCredit(v.initial_credit);
  assert.equal(started.enable('flow'), null);
  assert.equal(started.enable('tx').toString('hex'), fixture.ble.initial_credit_value);
  started.write(20);
});

test('net-v2 negative: a write beyond the credit closes the carrier connection', () => {
  const v = negative.credit_overrun;
  const { result, accepted } = runCredit(v);
  assert.equal(result, v.expected);
  assert.equal(accepted, v.accepted_bytes);
  const raised = new FlowCredit(v.initial_credit);
  raised.enable('tx');
  raised.enable('flow');
  raised.write(v.initial_credit);
  raised.grant(v.initial_credit + 20);
  raised.write(20);
  assert.throws(() => raised.grant(v.initial_credit), /credit decreased/);
});

test('net-v2 negative: every small-order X25519 share closes the handshake', () => {
  const secret = bytes(negative.x25519.secret);
  assert.equal(negative.x25519.small_order.length, 12);
  for (const { note, share: value, expected } of negative.x25519.small_order) {
    const peer = bytes(value);
    assert.equal(outcome(() => x25519(secret, peer)), expected, note);
    const u = BigInt(`0x${Buffer.from(peer).reverse().toString('hex')}`);
    if (u < P25519) {
      // Canonical small-order points give an all-zero Z for any secret.
      assert.equal(rawX25519(secret, peer), null, note);
      assert.equal(rawX25519(inputs.deviceEphemeral, peer), null, note);
    } else {
      assert.throws(() => canonicalShare(peer), /non-canonical/, note);
    }
  }
  for (const side of ['device', 'companion']) {
    const peer = bytes(negative.x25519.small_order[2].share);
    if (side === 'device') {
      assert.equal(outcome(() => device().open(handshake(0x20, inputs.deviceId, inputs.companionId, peer))), 'close');
    } else {
      const c = companion();
      assert.equal(outcome(() => c.challenge(handshake(0x21, peer, sess.keys.deckProof))), 'close');
    }
  }
});

test('net-v2 negative: a share with bit 255 set closes even where masking would succeed', () => {
  const { share: value, expected } = negative.x25519.bit_255_set;
  const peer = bytes(value);
  assert.equal(peer[31] & 0x80, 0x80);
  // RFC 7748 masking alone would accept it and agree on the honest Z.
  assert.deepEqual(rawX25519(inputs.clientEphemeral, peer), sess.Z);
  assert.equal(outcome(() => x25519(inputs.clientEphemeral, peer)), expected);
  assert.equal(outcome(() => companion().challenge(handshake(0x21, peer, sess.keys.deckProof))), 'close');
  assert.equal(outcome(() => device().open(handshake(0x20, inputs.deviceId, inputs.companionId,
    Buffer.from(share(inputs.clientEphemeral)).fill(0xff, 31, 32)))), 'close');
});
