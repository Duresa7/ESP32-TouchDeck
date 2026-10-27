import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createAppUpdater, parseUpdateError, type UpdateError, type UpdateInfo, type UpdateState } from './controller.ts';

const update: UpdateInfo = { version: '0.2.0', currentVersion: '0.1.0', notes: 'Fixture release' };

test('checking and downloading never install or restart; install requires a separate action', async () => {
  const states: UpdateState[] = [];
  let installs = 0;
  const updater = createAppUpdater({ changed: (state) => states.push(state), api: {
    check: async () => update,
    download: async (progress) => { progress({ downloaded: 16, total: 32 }); },
    install: async () => { installs++; },
  } });
  await updater.install();
  assert.equal(installs, 0);
  await updater.check();
  assert.equal(states.at(-1)?.kind, 'available');
  await updater.download();
  assert.equal(states.at(-1)?.kind, 'ready');
  assert.equal(installs, 0);
  assert(states.some((state) => state.kind === 'downloading' && state.progress.downloaded === 16));
  await updater.install();
  assert.equal(installs, 1);
});

test('offline checks and signature failure keep the app usable and never offer install', async () => {
  const states: UpdateState[] = [];
  let offline = true;
  const updater = createAppUpdater({ changed: (state) => states.push(state), api: {
    check: async () => { if (offline) throw { kind: 'network', message: 'offline fixture' } satisfies UpdateError; return update; },
    download: async () => { throw { kind: 'signature', message: 'signature fixture' } satisfies UpdateError; },
    install: async () => { assert.fail('unverified installer must not run'); },
  } });
  await updater.check();
  assert.deepEqual(states.at(-1), { kind: 'error', error: { kind: 'network', message: 'offline fixture' } });
  offline = false;
  await updater.check();
  await updater.download();
  assert.deepEqual(states.at(-1), { kind: 'error', error: { kind: 'signature', message: 'signature fixture' } });
  await updater.install();
});

test('permanent configuration and platform errors retain their kind instead of suggesting an offline retry', async () => {
  for (const kind of ['unconfigured', 'unsupported'] satisfies UpdateError['kind'][]) {
    const states: UpdateState[] = [];
    const error: UpdateError = { kind, message: `${kind} fixture` };
    const updater = createAppUpdater({ changed: (state) => states.push(state), api: {
      check: async () => { throw error; },
      download: async () => assert.fail('no update is available'),
      install: async () => assert.fail('no installer is available'),
    } });
    await updater.check();
    assert.deepEqual(states.at(-1), { kind: 'error', error });
    await updater.download();
    await updater.install();
  }
  assert.deepEqual(parseUpdateError({ kind: 'future', message: 'unknown' }), { kind: 'internal', message: '[object Object]' });
  assert.deepEqual(parseUpdateError(new Error('unexpected')), { kind: 'internal', message: 'Error: unexpected' });
});

test('current version and rejected transfer reservation are recoverable states', async () => {
  const states: UpdateState[] = [];
  let latest = true;
  const updater = createAppUpdater({ changed: (state) => states.push(state), api: {
    check: async () => latest ? null : update,
    download: async () => {},
    install: async () => { throw new Error('active transfer'); },
  } });
  await updater.check();
  assert.equal(states.at(-1)?.kind, 'current');
  latest = false;
  await updater.check();
  await updater.download();
  await assert.rejects(updater.install(), /active transfer/);
  assert.equal(states.at(-1)?.kind, 'ready');
});
