import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile, unlink } from 'node:fs/promises';
import { connect, devUiOrigin, revoke, runtimePath, restartMessage } from '../dist/companion.js';
import { fakeCompanion } from './fake-companion.mjs';

test('proof precedes mint, only window sessions are issued, and close revokes', async () => {
  const fake = await fakeCompanion();
  try {
    const session = await connect(fake.path, () => assert.fail('already running'));
    assert.equal(session.origin, fake.origin);
    assert.notEqual(session.token, fake.runtime.token);
    assert.deepEqual(fake.calls.map(c => c.path), ['/api/v1/auth/proof', '/api/v1/system/ping', '/api/v1/auth/sessions']);
    assert.equal(fake.calls[0].authorization, undefined);
    await revoke(session);
    assert.equal(fake.sessions.size, 0);
  } finally { await fake.close(); }
});

test('wrong proof never sends any token, including on retry with a stale runtime', async () => {
  const fake = await fakeCompanion({ wrongProof: true });
  let starts = 0;
  try {
    await assert.rejects(connect(fake.path, () => { starts++; }, 150), /verify/);
    assert.equal(starts, 1);
    assert.ok(fake.calls.every(c => !c.authorization && c.path === '/api/v1/auth/proof'));
  } finally { await fake.close(); }
});

test('missing runtime launches and accepts the newly published runtime', async () => {
  const fake = await fakeCompanion();
  try {
    await unlink(fake.path);
    let write;
    const session = await connect(fake.path, () => { write = writeFile(fake.path, JSON.stringify(fake.runtime)); });
    await write;
    assert.equal(session.origin, fake.origin);
  } finally { await fake.close(); }
});

test('unsupported API refuses to mint and requests restart', async () => {
  const fake = await fakeCompanion({ api: ['v2'] });
  try {
    await assert.rejects(connect(fake.path, () => assert.fail()), { message: restartMessage });
    assert.equal(fake.sessions.size, 0);
  } finally { await fake.close(); }
});

test('runtime paths honor overrides and platform contract', () => {
  assert.match(runtimePath({ TOUCHDECK_APP_DATA_DIR: 'scratch' }), /scratch[\\/]runtime[\\/]companion.json$/);
  assert.match(runtimePath({ XDG_RUNTIME_DIR: 'runtime' }, 'linux'), /runtime[\\/]touchdeck[\\/]companion.json$/);
  assert.match(runtimePath({ LOCALAPPDATA: 'local' }, 'win32'), /local[\\/]io.github.duresa7.touchdeck[\\/]runtime/);
});

test('the UI dev server override accepts only a loopback http origin', () => {
  assert.equal(devUiOrigin(undefined), undefined);
  assert.equal(devUiOrigin('http://127.0.0.1:5174/'), 'http://127.0.0.1:5174');
  for (const url of ['http://localhost:5174/', 'https://127.0.0.1:5174/', 'http://127.0.0.1/', 'http://127.0.0.1:5174/app',
    'http://127.0.0.1:5174/?x=1', 'http://user@127.0.0.1:5174/', 'http://evil.example:5174/', 'not a url']) {
    assert.throws(() => devUiOrigin(url), undefined, url);
  }
});
