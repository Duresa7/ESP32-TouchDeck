import { afterEach, describe, expect, test, vi } from 'vitest';
import type { ControlLayout, DeviceSummary, LinkState } from '../companion/api.ts';
import { createClient } from '../companion/client.ts';
import { createDevicesStore } from '../companion/devices.ts';
import { openEvents } from '../companion/events.ts';
import { createFakeCompanion } from '../companion/fake.ts';
import { createJobsStore, waitForJob } from '../companion/jobs.ts';
import { GIF_CAPS, sampleDevice } from '../companion/samples.ts';
import { windowSession } from '../companion/session.ts';
import { watchSends } from '../../screens/layout/editor.ts';
import { createEditorStore } from '../editor/store.ts';
import type { Layout } from '../layout/model.ts';
import { createControlsStore, noticeOf, observe, QUIET, sending, sent, type ControlWatch } from './controls.ts';

const CONNECTED: LinkState = { state: 'connected', port: 'COM3', since_ms: 0, protocol: 2 };
const LOST: LinkState = { state: 'lost', reason: 'unplugged', detail: '', at_ms: 1 };
const CHECKING: ControlLayout = { state: 'unavailable', reason: 'checking' };
const NO_COPY: ControlLayout = { state: 'no-pushed-copy', orientation: 'landscape', has_controls: true };
const DIFFERS: ControlLayout = { state: 'differs', orientation: 'landscape', has_controls: true };
const MATCHES: ControlLayout = { state: 'matches', orientation: 'landscape', has_controls: true };

const desk = (control_layout: ControlLayout, link: LinkState = CONNECTED): DeviceSummary => ({ ...sampleDevice('mock0001', 'Desk', link), control_layout });

const walk = (steps: readonly ((watch: ControlWatch) => ControlWatch)[], from = QUIET) => steps.reduce((watch, step) => step(watch), from);
const see = (control_layout: ControlLayout, link: LinkState = CONNECTED) => (watch: ControlWatch) => observe(watch, desk(control_layout, link));
const notice = (watch: ControlWatch, link: LinkState = CONNECTED) => noticeOf(watch, desk(CHECKING, link)).kind;

const ASSESSMENTS: ControlLayout[] = [
  { state: 'unavailable', reason: 'offline' },
  { state: 'unavailable', reason: 'checking' },
  { state: 'unavailable', reason: 'unsupported' },
  { state: 'unavailable', reason: 'read-failed' },
  ...(['no-pushed-copy', 'matches', 'differs'] as const).flatMap((state) => [true, false].map((has_controls) => ({ state, orientation: 'landscape' as const, has_controls }))),
];

describe('when a Device asks for a Send', () => {
  const cases = ASSESSMENTS.flatMap((status) => [true, false].map((online) => ({ status, online })));
  test.each(cases)('$status.state $status.reason $status.has_controls online $online', ({ status, online }) => {
    const link = online ? CONNECTED : LOST;
    const watch = observe(QUIET, desk(status, link));
    const asks = online && (status.state === 'no-pushed-copy' || status.state === 'differs') && status.has_controls;
    expect(noticeOf(watch, desk(status, link))).toEqual(asks ? { kind: 'needs-send', reason: status.state, orientation: 'landscape' } : { kind: 'none' });
  });

  test('keeps asking while the Companion checks again, and stops once it is offline, unsupported, unreadable or matching', () => {
    expect(notice(walk([see(NO_COPY), see(CHECKING)]))).toBe('needs-send');
    expect(notice(walk([see(CHECKING)]))).toBe('none');
    for (const end of [MATCHES, { ...NO_COPY, has_controls: false }, { state: 'unavailable', reason: 'unsupported' }, { state: 'unavailable', reason: 'read-failed' }] satisfies ControlLayout[]) {
      expect(notice(walk([see(NO_COPY), see(end)]))).toBe('none');
    }
    expect(notice(walk([see(NO_COPY), see(NO_COPY, LOST), see(CHECKING)]))).toBe('none');
    expect(noticeOf(walk([see(NO_COPY)]), desk(NO_COPY, LOST))).toEqual({ kind: 'none' });
  });
});

describe('Checking controls after a Send', () => {
  const asked = walk([see(NO_COPY)]);
  const send = (watch: ControlWatch) => sending(watch, { orientation: 'landscape', before: NO_COPY });

  test('shows only after the Job succeeds, and clears only when a fresh matches arrives', () => {
    const inFlight = walk([send, see(NO_COPY)], asked);
    expect(notice(inFlight)).toBe('needs-send');
    const succeeded = walk([(w) => sent(w, 'succeeded')], inFlight);
    expect(notice(succeeded)).toBe('checking');
    expect(notice(walk([see(NO_COPY)], succeeded)), 'the assessment from before the Send is stale').toBe('checking');
    const rechecking = walk([see(CHECKING)], succeeded);
    expect(notice(rechecking)).toBe('checking');
    expect(notice(walk([see(MATCHES)], rechecking))).toBe('none');
  });

  test('a fresh assessment that still asks for a Send brings the guidance back', () => {
    const afterReset = walk([send, see(CHECKING), (w) => sent(w, 'succeeded')], asked);
    expect(notice(afterReset)).toBe('checking');
    expect(noticeOf(walk([see(DIFFERS)], afterReset), desk(DIFFERS))).toEqual({ kind: 'needs-send', reason: 'differs', orientation: 'landscape' });
    expect(notice(walk([send, (w) => sent(w, 'succeeded'), see(DIFFERS)], asked)), 'a changed assessment is fresh').toBe('needs-send');
  });

  test('once the Companion has reset, an assessment equal to the one before the Send is fresh', () => {
    const needs = { kind: 'needs-send', reason: 'no-pushed-copy', orientation: 'landscape' };
    expect(noticeOf(walk([send, see(CHECKING), (w) => sent(w, 'succeeded'), see(NO_COPY)], asked), desk(NO_COPY))).toEqual(needs);
    expect(noticeOf(walk([send, (w) => sent(w, 'succeeded'), see(CHECKING), see(NO_COPY)], asked), desk(NO_COPY))).toEqual(needs);
  });

  test('losing the link while sending or checking drops the check, and the reconnect asks again', () => {
    const offline = see({ state: 'unavailable', reason: 'offline' }, LOST);
    const reconnect = [see(CHECKING), see(NO_COPY)];
    const whileChecking = walk([send, (w) => sent(w, 'succeeded'), offline], asked);
    expect(noticeOf(whileChecking, desk(CHECKING, LOST))).toEqual({ kind: 'none' });
    expect(notice(walk([see(CHECKING)], whileChecking))).toBe('none');
    expect(notice(walk(reconnect, whileChecking))).toBe('needs-send');
    const whileSending = walk([send, offline, (w) => sent(w, 'succeeded')], asked);
    expect(notice(walk([see(CHECKING)], whileSending))).toBe('none');
    expect(notice(walk(reconnect, whileSending))).toBe('needs-send');
    expect(notice(walk([see(CHECKING), see(MATCHES)], whileSending))).toBe('none');
  });

  test('a check that ends unread says so until a later assessment decides', () => {
    for (const reason of ['read-failed', 'unsupported'] as const) {
      const unread = see({ state: 'unavailable', reason });
      const ended = walk([send, see(CHECKING), (w) => sent(w, 'succeeded'), unread], asked);
      expect(noticeOf(ended, desk(CHECKING)), reason).toEqual({ kind: 'unchecked', reason, orientation: 'landscape' });
      expect(notice(walk([send, see(CHECKING), unread, (w) => sent(w, 'succeeded')], asked)), `${reason} while the Job runs`).toBe('unchecked');
      expect(notice(walk([send, unread], asked)), `${reason} while the Job runs keeps the guidance`).toBe('needs-send');
      expect(notice(walk([send, unread, (w) => sent(w, 'failed')], asked)), `${reason} and a failed Send keep the guidance`).toBe('needs-send');
      expect(notice(walk([see(CHECKING)], ended))).toBe('checking');
      expect(notice(walk([see(MATCHES)], ended))).toBe('none');
      expect(notice(walk([see(NO_COPY)], ended))).toBe('needs-send');
      expect(noticeOf(ended, desk(CHECKING, LOST))).toEqual({ kind: 'none' });
    }
  });

  test('a Layout without controls after the Send has nothing to check', () => {
    for (const end of [MATCHES, NO_COPY, DIFFERS].map((status) => ({ ...status, has_controls: false }))) {
      expect(notice(walk([send, see(CHECKING), (w) => sent(w, 'succeeded'), see(end)], asked)), end.state).toBe('none');
    }
  });

  test('a failed Send keeps the guidance', () => {
    expect(notice(walk([send, see(CHECKING), (w) => sent(w, 'failed')], asked))).toBe('needs-send');
    expect(notice(walk([send, (w) => sent(w, 'failed'), see(CHECKING), see(NO_COPY)], asked))).toBe('needs-send');
  });

  test('sending the other orientation never claims to check the active one', () => {
    const other = walk([(w) => sending(w, { orientation: 'portrait', before: NO_COPY }), (w) => sent(w, 'succeeded')], asked);
    expect(other).toBe(asked);
    expect(notice(walk([(w) => sending(w, { orientation: 'landscape', before: MATCHES }), (w) => sent(w, 'succeeded')]))).toBe('none');
  });
});

const media: Layout = {
  schema: 2,
  name: 'Media',
  orientation: 'landscape',
  pages: [
    {
      kind: 'grid',
      id: 'media',
      title: 'Media',
      rows: 2,
      cols: 4,
      tiles: [
        { id: 'volume', label: 'Volume', row: 0, col: 0, rowSpan: 1, colSpan: 2, icon: null, render: 'bar', streams: { value: 'number', muted: 'number' }, input: true, action: { type: 'widget', source: 'volume', field: 'level', account: '' } },
      ],
    },
  ],
};

describe('a Send through the editor on the fake Companion', () => {
  afterEach(() => vi.unstubAllGlobals());

  test('asks, checks after the Send, and clears when the Companion reports matches', async () => {
    const fake = createFakeCompanion({ devices: [{ ...desk(CHECKING), capabilities: GIF_CAPS }], layouts: { mock0001: media }, jobMs: 5 });
    vi.stubGlobal('fetch', fake.fetch);
    const companion = createClient('http://127.0.0.1:51234', windowSession(fake.bridge));
    const devices = createDevicesStore();
    const jobs = createJobsStore();
    const controls = createControlsStore(devices);
    const events = openEvents({ url: 'ws://127.0.0.1:51234/api/v1/events', session: windowSession(fake.bridge), store: devices, jobs, open: fake.openSocket });
    const editor = createEditorStore({ companion, storage: null, wait: (job) => waitForJob(jobs, companion, job, { pollMs: 10 }) });
    watchSends(editor, controls);
    const shown = () => noticeOf(controls.getState().watched.mock0001, devices.getState().devices[0]).kind;
    await vi.waitFor(() => expect(devices.getState().status).toBe('live'));

    fake.controls.set('mock0001', NO_COPY);
    await vi.waitFor(() => expect(shown()).toBe('needs-send'));
    editor.open({ id: 'mock0001', name: 'Desk' });
    await editor.load();
    fake.controls.hold('mock0001');
    await editor.push();
    expect(editor.getState().status?.event).toBe('pushed');
    await vi.waitFor(() => expect(devices.getState().devices[0].control_layout).toEqual(CHECKING));
    expect(shown()).toBe('checking');
    fake.controls.release('mock0001');
    await vi.waitFor(() => expect(devices.getState().devices[0].control_layout).toEqual(MATCHES));
    expect(shown()).toBe('none');
    events.close();
  });
});
