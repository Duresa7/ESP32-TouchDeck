import { createConnection } from 'node:net';
import { once } from 'node:events';
import { randomBytes } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { createInterface } from 'node:readline/promises';
import { Writable } from 'node:stream';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import {
  concat, fixed, equal, requireThat, handshake, parseHandshake, identity,
  clientPublic, clientSecret, pairKeys, acceptKey, commitProof, storedProof,
  sessionKeys, frame, validateFrame, RecordSender, RecordReceiver,
} from './protocol.mjs';

export const now = () => performance.now();

export async function before(promise, deadline, expire) {
  const delay = deadline - now();
  if (delay <= 0) {
    expire();
    throw new Error('deadline expired');
  }
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => {
        expire();
        reject(new Error('deadline expired'));
      }, delay);
    })]);
  } finally {
    clearTimeout(timer);
  }
}

export class WireSocket {
  constructor(socket) {
    this.socket = socket;
    this.socket.setNoDelay(true);
    this.iterator = socket[Symbol.asyncIterator]();
    this.buffer = Buffer.alloc(0);
  }

  static async connect(host, port = 47820) {
    const socket = createConnection({ host, port });
    await before(once(socket, 'connect'), now() + 10000, () => socket.destroy());
    return new WireSocket(socket);
  }

  async exact(size, deadline) {
    try {
      requireThat(now() < deadline, 'deadline expired');
      while (this.buffer.length < size) {
        const part = await before(this.iterator.next(), deadline, () => this.close());
        requireThat(!part.done, 'connection ended inside item');
        this.buffer = concat(this.buffer, part.value);
      }
      requireThat(now() < deadline, 'deadline expired');
      const result = this.buffer.subarray(0, size);
      this.buffer = this.buffer.subarray(size);
      return result;
    } catch (error) {
      this.close();
      throw error;
    }
  }

  async read(min, max, deadline) {
    try {
      const prefix = await this.exact(2, deadline);
      const size = prefix.readUInt16BE();
      requireThat(size >= min && size <= max, 'invalid item length');
      return concat(prefix, await this.exact(size, deadline));
    } catch (error) {
      this.close();
      throw error;
    }
  }

  async send(wire, deadline) {
    try {
      await before(new Promise((resolveWrite, reject) => {
        this.socket.write(wire, (error) => error ? reject(error) : resolveWrite());
      }), deadline, () => this.close());
    } catch (error) {
      this.close();
      throw error;
    }
  }

  close() {
    this.socket.destroy();
  }

  async finish(deadline) {
    this.socket.end();
    this.buffer = Buffer.alloc(0);
    try {
      while (!(await before(this.iterator.next(), deadline, () => this.close())).done) {}
    } catch {
      this.close();
    }
  }
}

async function receiveHandshake(wire, kind, deadline) {
  const item = await wire.read(3, 512, deadline);
  const decoded = parseHandshake(item);
  if (decoded.kind === 0x7f) {
    const names = ['unknown', 'busy', 'version', 'args', 'full', 'auth', 'expired', 'cancelled', 'storage'];
    throw new Error(`handshake refused (${names[decoded.fields[0]]})`);
  }
  requireThat(decoded.kind === kind, 'unexpected handshake kind');
  return { item, fields: decoded.fields };
}

export async function pairClient({ host, port = 47820, appId = randomBytes(16),
  appName = 'Reference client', getCode, savePending = async () => {} }) {
  const wire = await WireSocket.connect(host, port);
  let a;
  try {
    const name = Buffer.from(appName);
    const request = handshake(0x10, fixed(appId, 16), Buffer.from([name.length]), name);
    const initialDeadline = now() + 10000;
    await wire.send(request, initialDeadline);
    const offer = await receiveHandshake(wire, 0x11, initialDeadline);
    const deckId = Buffer.from(offer.fields.subarray(0, 6));
    const windowDeadline = now() + offer.fields.readUInt32BE(22);
    const input = new AbortController();
    const code = await before(Promise.resolve().then(() => getCode({ signal: input.signal })), windowDeadline, () => {
      input.abort();
      wire.close();
    });
    requireThat(typeof code === 'string' && /^[0-9]{6}$/.test(code), 'invalid code');
    a = randomBytes(32);
    while (a.every((byte) => byte === 0)) a = randomBytes(32);
    const start = handshake(0x12, clientPublic(a), randomBytes(32));
    const deadline = Math.min(windowDeadline, now() + 10000);
    await wire.send(start, deadline);
    const challenge = await receiveHandshake(wire, 0x13, deadline);
    const salt = challenge.fields.subarray(0, 16);
    const B = challenge.fields.subarray(16, 400);
    const secret = clientSecret({ I: identity(deckId, appId), code, salt, a, B });
    const keys = pairKeys(secret.K, request, offer.item, start, challenge.item);
    await wire.send(handshake(0x14, keys.clientProof), deadline);
    const acceptance = await receiveHandshake(wire, 0x15, deadline);
    const ltk = acceptKey(keys, acceptance.item);
    const credential = { deckId, appId: Buffer.from(appId), appName, ltk };
    await before(Promise.resolve().then(() => savePending(credential)), deadline, () => wire.close());
    await wire.send(handshake(0x16, commitProof(keys, ltk)), deadline);
    const stored = await receiveHandshake(wire, 0x17, deadline);
    requireThat(equal(stored.fields, storedProof(keys, ltk)), 'bad stored proof');
    return credential;
  } finally {
    a?.fill(0);
    wire.close();
  }
}

export async function connectSession({ host, port = 47820, credential, onEvent = () => {} }) {
  const wire = await WireSocket.connect(host, port);
  let session;
  try {
    const deadline = now() + 10000;
    const clientNonce = randomBytes(32);
    const { deckId, appId, ltk } = credential;
    await wire.send(handshake(0x20, deckId, appId, clientNonce), deadline);
    const challenge = await receiveHandshake(wire, 0x21, deadline);
    const keys = sessionKeys({ deckId, appId, ltk, clientNonce, deckNonce: challenge.fields.subarray(0, 32) });
    requireThat(equal(challenge.fields.subarray(32), keys.deckProof), 'bad session deck proof');
    await wire.send(handshake(0x22, keys.clientProof), deadline);
    const ready = await receiveHandshake(wire, 0x23, deadline);
    requireThat(equal(ready.fields, keys.readyProof), 'bad session ready proof');
    session = new LinkSession(wire, keys, onEvent);
    const hello = await session.request({ m: 'hello' });
    requireThat(hello.ok === true && hello.product === 'touchdeck' && hello.proto === 1, 'invalid hello');
    return session;
  } catch (error) {
    session?.close(error);
    wire.close();
    throw error;
  }
}

export class LinkSession {
  constructor(wire, keys, onEvent) {
    this.wire = wire;
    this.sender = new RecordSender(keys.c2d, 0);
    this.receiver = new RecordReceiver(keys.d2c, 1);
    this.pending = new Map();
    this.transfers = new Map();
    this.nextSequence = 1;
    this.closed = false;
    this.onEvent = onEvent;
    this.writes = Promise.resolve();
    this.reading = this.receive().catch((error) => this.close(error));
    this.keepalive = setInterval(() => {
      this.request({ m: 'ping' }).catch((error) => this.close(error));
    }, 2000);
  }

  send(body) {
    const pendingWrite = this.writes.then(() => {
      requireThat(!this.closed, 'session closed');
      return this.wire.send(this.sender.seal(body), now() + 5000);
    });
    this.writes = pendingWrite.catch((error) => this.close(error));
    return pendingWrite;
  }

  request(message) {
    if (this.closed) return Promise.reject(new Error('session closed'));
    let seq = this.nextSequence;
    while (this.pending.has(seq)) {
      seq = seq % 255 + 1;
      if (seq === this.nextSequence) return Promise.reject(new Error('request queue full'));
    }
    this.nextSequence = seq % 255 + 1;
    let body;
    try { body = frame(message, { seq }); }
    catch (error) { return Promise.reject(error); }
    return new Promise((resolveReply, reject) => {
      const timer = setTimeout(() => this.close(new Error('response deadline expired')), 5000);
      this.pending.set(seq, { message, resolve: resolveReply, reject, timer });
      this.send(body).catch((error) => this.close(error));
    });
  }

  async receive() {
    while (!this.closed) {
      const wire = await this.wire.read(24, 1048, now() + 5000);
      const body = this.receiver.open(wire);
      const decoded = validateFrame(body);
      if (decoded.type === 2 && decoded.flags === 0) {
        const xfer = decoded.payload[0];
        const transfer = this.transfers.get(xfer);
        requireThat(transfer !== undefined, 'unsolicited transfer');
        const index = decoded.payload.readUInt16LE(1);
        if (index === transfer.next) {
          const chunk = Buffer.from(decoded.payload.subarray(3));
          requireThat(transfer.size + chunk.length <= transfer.bytes, 'transfer overflow');
          transfer.parts.push(chunk);
          transfer.size += chunk.length;
          transfer.next++;
        }
        const ack = Buffer.alloc(3);
        ack[0] = xfer;
        ack.writeUInt16LE(transfer.next, 1);
        await this.send(frame(ack, { type: 3, flags: 1, seq: decoded.seq }));
        if (transfer.size === transfer.bytes) transfer.resolve(concat(...transfer.parts));
        continue;
      }
      if (decoded.type !== 1) continue;
      let message;
      try { message = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(decoded.payload)); }
      catch { continue; }
      if (decoded.flags === 2) this.onEvent(message);
      if (decoded.flags !== 1) continue;
      const pending = this.pending.get(decoded.seq);
      if (!pending || message?.m !== pending.message.m) continue;
      if (pending.message.m === 'layout.get' && message.ok) {
        requireThat(Number.isInteger(message.xfer) && message.xfer >= 0 && message.xfer <= 255, 'invalid transfer id');
        requireThat(Number.isInteger(message.bytes) && message.bytes > 0 && message.bytes <= 32768, 'invalid transfer size');
        const transfer = { bytes: message.bytes, size: 0, next: 0, parts: [] };
        transfer.done = new Promise((resolveTransfer, reject) => {
          transfer.resolve = resolveTransfer;
          transfer.reject = reject;
        });
        transfer.done.catch(() => {});
        this.transfers.set(message.xfer, transfer);
      }
      this.pending.delete(decoded.seq);
      clearTimeout(pending.timer);
      pending.resolve(message);
    }
  }

  async getLayout(orientation = 'landscape') {
    const reply = await this.request({ m: 'layout.get', orientation });
    requireThat(reply.ok === true, 'layout.get refused');
    const transfer = this.transfers.get(reply.xfer);
    try {
      const data = await before(transfer.done, now() + 10000, () => this.close(new Error('transfer deadline expired')));
      requireThat(crc32(data) === reply.crc32, 'layout CRC32 mismatch');
      const end = await this.request({ m: 'xfer.end', xfer: reply.xfer });
      requireThat(end.ok === true, 'xfer.end refused');
      return data;
    } finally {
      this.transfers.delete(reply.xfer);
    }
  }

  close(error = new Error('session closed')) {
    if (this.closed) return;
    this.closed = true;
    clearInterval(this.keepalive);
    this.wire.close();
    this.sender.close();
    this.receiver.close();
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pending.clear();
    for (const transfer of this.transfers.values()) transfer.reject(error);
    this.transfers.clear();
  }
}

export function crc32(data) {
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

async function promptCode({ signal }) {
  requireThat(process.stdin.isTTY, 'interactive code input needs a terminal; use the module API for automation');
  const muted = new Writable({ write(_chunk, _encoding, callback) { callback(); } });
  const input = createInterface({ input: process.stdin, output: muted, terminal: true });
  process.stdout.write('Enter the six-digit code shown on the deck. Input is hidden.\n');
  try { return await input.question('', { signal }); }
  finally { input.close(); }
}

async function main() {
  const host = process.argv[2];
  requireThat(host, 'usage: node client.mjs <host> [widget-tile-id]');
  let pending;
  let session;
  try {
    const credential = await pairClient({ host, getCode: promptCode, savePending: async (entry) => { pending = entry; } });
    session = await connectSession({ host, credential });
    requireThat((await session.request({ m: 'ping' })).ok === true, 'ping refused');
    const layout = await session.getLayout();
    process.stdout.write(`Authenticated hello and ping passed. Retrieved ${layout.length} layout bytes.\n`);
    if (process.argv[3]) {
      const reply = await session.request({ m: 'widget.set', tile: process.argv[3], value: '42', caption: 'Network test' });
      requireThat(reply.ok === true, 'widget.set refused');
      process.stdout.write('widget.set passed.\n');
    }
    await session.wire.finish(now() + 100);
    session.close();
    session = await connectSession({ host, credential });
    requireThat((await session.request({ m: 'ping' })).ok === true, 'reconnect ping refused');
    process.stdout.write('Reconnect without pairing passed. The reference keeps its credential in memory only.\n');
  } finally {
    session?.close();
    pending?.ltk.fill(0);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(() => {
    process.stderr.write('Network reference check failed. No credential values were logged.\n');
    process.exitCode = 1;
  });
}
