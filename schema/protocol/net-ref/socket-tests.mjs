import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:net';
import { once, EventEmitter } from 'node:events';
import { randomBytes } from 'node:crypto';
import {
  bytes, concat, equal, handshake, parseHandshake, u32, identity,
  serverSecret, pairKeys, wrapKey, commitProof, storedProof, sessionKeys,
  frame, validateFrame, RecordSender, RecordReceiver,
} from './protocol.mjs';
import { WireSocket, now, pairClient, connectSession, crc32 } from './client.mjs';
import { inputs } from './vectors.mjs';

async function localDeck() {
  const pairings = new Map();
  const sockets = new Set();
  const calls = [];
  const events = new EventEmitter();
  const failures = [];
  const layout = Buffer.from(JSON.stringify({ name: 'Reference layout', tiles: [] }));
  let owner;
  async function receive(wire, kind) {
    const item = await wire.read(3, 512, now() + 10000);
    return { item, fields: parseHandshake(item, kind).fields };
  }
  async function serve(socket) {
    const wire = new WireSocket(socket);
    try {
      if (owner && !owner.destroyed) {
        await wire.send(handshake(0x7f, Buffer.from([1])), now() + 100);
        return;
      }
      owner = socket;
      const first = await wire.read(3, 512, now() + 10000);
      const decoded = parseHandshake(first);
      if (decoded.kind === 0x10) {
        const appId = decoded.fields.subarray(0, 16);
        const offer = handshake(0x11, inputs.deckId, randomBytes(16), u32(120000));
        await wire.send(offer, now() + 10000);
        const start = await receive(wire, 0x12);
        const salt = randomBytes(16);
        const b = randomBytes(32);
        const secret = serverSecret({ I: identity(inputs.deckId, appId), code: inputs.code,
          salt, b, A: start.fields.subarray(0, 384) });
        const challenge = handshake(0x13, salt, secret.B, randomBytes(32));
        const keys = pairKeys(secret.K, first, offer, start.item, challenge);
        await wire.send(challenge, now() + 10000);
        const proof = await receive(wire, 0x14);
        if (!equal(proof.fields, keys.clientProof)) {
          await wire.send(handshake(0x7f, Buffer.from([5])), now() + 10000);
          return;
        }
        const ltk = randomBytes(32);
        await wire.send(handshake(0x15, keys.deckProof, wrapKey(keys, ltk)), now() + 10000);
        const commit = await receive(wire, 0x16);
        assert.deepEqual(commit.fields, commitProof(keys, ltk));
        pairings.set(appId.toString('hex'), { deckId: inputs.deckId, appId, ltk });
        await wire.send(handshake(0x17, storedProof(keys, ltk)), now() + 10000);
        return;
      }
      assert.equal(decoded.kind, 0x20);
      const appId = decoded.fields.subarray(6, 22);
      const credential = pairings.get(appId.toString('hex'));
      if (!credential || !equal(decoded.fields.subarray(0, 6), inputs.deckId)) {
        await wire.send(handshake(0x7f, Buffer.from([5])), now() + 10000);
        return;
      }
      const deckNonce = randomBytes(32);
      const keys = sessionKeys({ ...credential, deckNonce, clientNonce: decoded.fields.subarray(22) });
      await wire.send(handshake(0x21, deckNonce, keys.deckProof), now() + 10000);
      const proof = await receive(wire, 0x22);
      assert.deepEqual(proof.fields, keys.clientProof);
      await wire.send(handshake(0x23, keys.readyProof), now() + 10000);
      const sender = new RecordSender(keys.d2c, 1);
      const receiver = new RecordReceiver(keys.c2d, 0);
      while (!socket.destroyed) {
        const body = validateFrame(receiver.open(await wire.read(24, 1048, now() + 5000)));
        if (body.type === 3) {
          calls.push('ack');
          continue;
        }
        const message = JSON.parse(body.payload.toString());
        calls.push(message.m);
        events.emit(message.m);
        const reply = { m: message.m, ok: true };
        if (message.m === 'hello') Object.assign(reply, { product: 'touchdeck', proto: 1 });
        if (message.m === 'ping') Object.assign(reply, { t: 1234 });
        if (message.m === 'layout.get') Object.assign(reply, { xfer: 7, bytes: layout.length, crc32: crc32(layout) });
        const response = sender.seal(frame(reply, { seq: body.seq, flags: 1 }));
        if (message.m === 'hello') {
          await wire.send(response.subarray(0, 1), now() + 5000);
          await wire.send(response.subarray(1, 5), now() + 5000);
          await wire.send(response.subarray(5), now() + 5000);
        } else await wire.send(response, now() + 5000);
        if (message.m === 'layout.get') {
          const chunk = concat(Buffer.from([7, 0, 0]), layout);
          await wire.send(sender.seal(frame(chunk, { type: 2, seq: 91 })), now() + 5000);
        }
        if (message.m === 'widget.set') {
          await wire.send(sender.seal(frame({ m: 'press', tile: 'test', seq: 1 }, { seq: 0, flags: 2 })), now() + 5000);
        }
      }
    } catch (error) {
      if (!/connection ended|aborted|ECONNRESET/.test(error.message)) failures.push(error);
    } finally {
      if (owner === socket) owner = undefined;
      await wire.finish(now() + 100);
    }
  }
  const server = createServer((socket) => {
    sockets.add(socket);
    socket.once('close', () => sockets.delete(socket));
    serve(socket).catch((error) => failures.push(error));
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return {
    host: '127.0.0.1', port: server.address().port, calls, events, pairings, failures, layout,
    async close() {
      for (const socket of sockets) socket.destroy();
      await new Promise((done) => server.close(done));
    },
  };
}

test('TCP reference pairs, runs hello/ping/layout/widget/event, keeps alive, refuses a second connection, and reconnects', { timeout: 10000 }, async (t) => {
  const deck = await localDeck();
  t.after(() => deck.close());
  let saved;
  const credential = await pairClient({ ...deck, getCode: async () => inputs.code,
    savePending: async (entry) => { saved = entry; } });
  assert.equal(saved, credential);
  assert.equal(deck.pairings.size, 1);
  let receiveEvent;
  const event = new Promise((resolveEvent) => { receiveEvent = resolveEvent; });
  const session = await connectSession({ ...deck, credential, onEvent: receiveEvent });
  t.after(() => session.close());
  assert.equal((await session.request({ m: 'ping' })).ok, true);
  assert.deepEqual(await session.getLayout(), deck.layout);
  assert.equal((await session.request({ m: 'widget.set', tile: 'test', value: '42' })).ok, true);
  assert.equal((await event).m, 'press');
  await once(deck.events, 'ping');
  await assert.rejects(connectSession({ ...deck, credential }), /busy/);
  await session.wire.finish(now() + 100);
  session.close();
  const again = await connectSession({ ...deck, credential });
  assert.equal((await again.request({ m: 'ping' })).ok, true);
  again.close();
  assert.ok(deck.calls.includes('ack'));
  assert.ok(deck.calls.includes('xfer.end'));
  assert.equal(deck.calls.filter((call) => call === 'hello').length, 2);
  assert.deepEqual(deck.failures, []);
});

test('a wrong code over TCP creates no stored credential', async (t) => {
  const deck = await localDeck();
  t.after(() => deck.close());
  let saved = false;
  await assert.rejects(pairClient({ ...deck, getCode: async () => '654321',
    savePending: async () => { saved = true; } }), /auth/);
  assert.equal(saved, false);
  assert.equal(deck.pairings.size, 0);
  assert.deepEqual(deck.failures, []);
});

test('TCP reader handles coalesced items, rejects oversized lengths, and closes on EOF', async (t) => {
  const peers = [];
  const server = createServer((socket) => peers.shift()(socket));
  t.after(() => new Promise((done) => server.close(done)));
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const port = server.address().port;
  const first = handshake(0x7f, Buffer.from([1]));
  const second = handshake(0x7f, Buffer.from([5]));
  peers.push((socket) => socket.end(concat(first, second)));
  const wire = await WireSocket.connect('127.0.0.1', port);
  assert.deepEqual(await wire.read(3, 512, now() + 1000), first);
  assert.deepEqual(await wire.read(3, 512, now() + 1000), second);
  await assert.rejects(wire.read(3, 512, now() + 1000), /ended/);
  peers.push((socket) => socket.end(bytes('ffff')));
  const oversized = await WireSocket.connect('127.0.0.1', port);
  await assert.rejects(oversized.read(3, 512, now() + 1000), /length/);
  peers.push((socket) => socket.end(first.subarray(0, -1)));
  const truncated = await WireSocket.connect('127.0.0.1', port);
  await assert.rejects(truncated.read(3, 512, now() + 1000), /ended/);
});

test('TCP item deadline does not reset when another fragment arrives', async (t) => {
  const server = createServer((socket) => {
    socket.write(bytes('0003'));
    socket.once('close', () => clearInterval(timer));
    const timer = setInterval(() => socket.write(Buffer.from([0x7f])), 50);
  });
  t.after(() => new Promise((done) => server.close(done)));
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const wire = await WireSocket.connect('127.0.0.1', server.address().port);
  await assert.rejects(wire.read(3, 512, now() + 80), /deadline|aborted/);
  assert.equal(wire.socket.destroyed, true);
});
