import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createAppUpdater, type UpdateInfo, type UpdateState } from './controller.ts';

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
    check: async () => { if (offline) throw new Error('offline fixture'); return update; },
    download: async () => { throw new Error('signature fixture'); },
    install: async () => { assert.fail('unverified installer must not run'); },
  } });
  await updater.check();
  assert.equal(states.at(-1)?.kind, 'error');
  offline = false;
  await updater.check();
  await updater.download();
  assert.equal(states.at(-1)?.kind, 'error');
  await updater.install();
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
