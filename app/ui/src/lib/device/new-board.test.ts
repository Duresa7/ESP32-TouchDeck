import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import type { Job } from '../../companion/api.ts';
import { createClient, type Companion } from '../../companion/client.ts';
import { createFakeCompanion, type FakeCompanion } from '../../companion/fake.ts';
import { newBoard } from '../../companion/fake-new-boards.ts';
import { createJobsStore, type JobsStore } from '../../companion/jobs.ts';
import { windowSession } from '../../companion/session.ts';
import { viewOf } from './firmware.ts';
import { boardStep, createBoardInstall, DECLINED, INSTALL_FAILED, installWords, type BoardInstallStore } from './new-board.ts';

const BASE = 'http://127.0.0.1:51234';
const BOARD = { id: 'mock00ff', device: 'the board', version: '' };

function job(change: Partial<Job>): Job {
  return {
    id: 'job_install000000001',
    kind: 'firmware.install',
    scope: { kind: 'new-board', id: 'mock00ff' },
    state: 'running',
    progress: { phase: 'checking' },
    result: null,
    error: null,
    cancellable: true,
    created_ms: 0,
    finished_ms: null,
    rev: 1,
    ...change,
  };
}

const failed = (code: string) => job({ state: 'failed', error: { code, message: `Failed with ${code}.`, details: null }, finished_ms: 1 });
const view = (from: Job) => viewOf(from, BOARD, installWords);

describe('the New board steps', () => {
  test('found, before anything is asked', () => {
    expect(boardStep({ step: 'idle' })).toBe('detected');
  });

  test('waiting for the confirmation on this PC', () => {
    expect(view(job({ state: 'waiting_for_confirmation' }))).toEqual({ step: 'waiting', text: 'Confirm on this PC to continue.', raw: 'Waiting for confirmation' });
    expect(boardStep(view(job({ state: 'waiting_for_confirmation' })))).toBe('waiting');
  });

  test.each([
    [{ phase: 'checking' as const }, 'Installing Firmware…', null],
    [{ phase: 'recovering' as const, pct: 60 }, 'Installing Firmware… 60%', 60],
    [{ phase: 'sending' as const, pct: 33.6 }, 'Installing Firmware… 34%', 33.6],
    [{ phase: 'rebooting' as const }, 'Installing Firmware…', null],
  ])('installing %j reads %j', (progress, text, pct) => {
    const running = view(job({ progress }));
    expect(running).toMatchObject({ step: 'running', text, pct });
    expect(boardStep(running)).toBe('installing');
  });

  test('done', () => {
    const done = view(job({ state: 'succeeded', result: { message: 'Firmware installed', version: '1.5.0' }, finished_ms: 1 }));
    expect(done).toEqual({ step: 'done', text: 'Firmware installed', raw: 'Firmware installed', version: '1.5.0' });
    expect(boardStep(done)).toBe('done');
  });

  test('declined on this PC', () => {
    const declined = view(failed('firmware.declined'));
    expect(declined).toMatchObject({ step: 'failed', text: DECLINED, code: 'firmware.declined' });
    expect(boardStep(declined)).toBe('declined');
  });

  test.each(['firmware.failed', 'new_board.not_found', 'device.busy', 'job.cancelled', 'internal'])('%s reads as a failed install', (code) => {
    const broken = view(failed(code));
    expect(broken).toMatchObject({ step: 'failed', text: INSTALL_FAILED, code });
    expect(boardStep(broken)).toBe('failed');
  });
});

describe('installing on a New board', () => {
  let fake: FakeCompanion;
  let companion: Companion;
  let jobs: JobsStore;
  const installs: BoardInstallStore[] = [];

  function install(id = 'mock00ff') {
    const store = createBoardInstall({ companion, jobs, board: { id }, pollMs: 5 });
    installs.push(store);
    return store;
  }

  const step = (store: BoardInstallStore) => boardStep(store.getState().view);

  beforeEach(() => {
    fake = createFakeCompanion({ devices: [], newBoards: [newBoard('mock00ff', 'COM7')], jobMs: 2 });
    vi.stubGlobal('fetch', fake.fetch);
    companion = createClient(BASE, windowSession(fake.bridge));
    jobs = createJobsStore();
  });

  afterEach(() => {
    for (const store of installs.splice(0)) store.dispose();
    vi.unstubAllGlobals();
  });

  test('nothing is written until the click, then it waits for this PC to confirm', async () => {
    const store = install();
    expect(step(store)).toBe('detected');
    expect(fake.newBoards.jobs()).toEqual([]);
    await store.start('bundled');
    expect(step(store)).toBe('waiting');
    expect(fake.newBoards.confirming('mock00ff')).toBe(true);
  });

  test('after the confirmation it installs, then the board becomes a Device under the same id', async () => {
    const store = install();
    await store.start('bundled');
    fake.newBoards.answer('mock00ff', 'accept');
    await vi.waitFor(() => expect(step(store)).toBe('installing'));
    await vi.waitFor(() => expect(step(store)).toBe('done'));
    expect(store.getState().view).toMatchObject({ text: 'Firmware installed', version: '1.5.0' });
    expect(fake.newBoards.boards()).toEqual([]);
    await vi.waitFor(() => expect(companion.devices.get({ device: 'mock00ff' })).resolves.toMatchObject({ setup: 'needs-setup', name: null }), { timeout: 2000 });
  });

  test('declining on this PC is explained and leaves the board as it was', async () => {
    const store = install();
    await store.start('bundled');
    fake.newBoards.answer('mock00ff', 'decline');
    await vi.waitFor(() => expect(step(store)).toBe('declined'));
    expect(store.getState().view).toMatchObject({ text: DECLINED });
    expect(fake.newBoards.boards().map((b) => b.id)).toEqual(['mock00ff']);
  });

  test('a failed install says what to try, and Retry asks again', async () => {
    fake.newBoards.outcome('mock00ff', 'fail');
    const store = install();
    await store.start('bundled');
    fake.newBoards.answer('mock00ff', 'accept');
    await vi.waitFor(() => expect(step(store)).toBe('failed'));
    expect(store.getState().view).toMatchObject({ text: INSTALL_FAILED, code: 'firmware.failed' });
    expect(store.getState().log.at(-1)).toBe('Failed: The update failed.');
    fake.newBoards.outcome('mock00ff', 'succeed');
    await store.start('bundled');
    expect(step(store)).toBe('waiting');
    fake.newBoards.answer('mock00ff', 'accept');
    await vi.waitFor(() => expect(step(store)).toBe('done'));
  });

  test('a board that was unplugged fails at once', async () => {
    const store = install('mock00fe');
    await store.start('bundled');
    expect(store.getState().view).toMatchObject({ step: 'failed', code: 'new_board.not_found', text: INSTALL_FAILED });
  });
});
