import { connectFirstDeck, mockAction, waitForDot } from '../deck.ts';
import { note, shot } from '../report.ts';

// Every key here is F13 to F24, Shift with F15, or a media key in a pair that cancels.
const F13 = 104;
const F14 = 105;
const F15 = 106;
const F16 = 107;
const F17 = 108;
const F18 = 109;
const SHIFT = 2;
const VOLUME_UP = 233;
const VOLUME_DOWN = 234;

type Emitted = Record<string, unknown>;
const kbd = (mods: number, keys: number[]): Emitted => ({ m: 'key.report', type: 'kbd', mods, keys });
const con = (usage: number): Emitted => ({ m: 'key.report', type: 'con', usage });

function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.execute(
    async (name, payload) => {
      const tauri = (window as unknown as { __TAURI__: { core: { invoke: (cmd: string, args: object) => Promise<unknown> } } }).__TAURI__;
      return tauri.core.invoke(name, payload);
    },
    command,
    args,
  ) as Promise<T>;
}

async function emit(events: Emitted[], repeat = 1, everyMs = 30) {
  if (!fake) await holdFocus();
  await invoke<void>('e2e_emit', { events, repeat, everyMs });
}

/** Keys go to the foreground window, so make sure that is this window with the monitor focused. */
async function holdFocus() {
  const ready = () =>
    browser.execute(() => document.hasFocus() && document.activeElement?.getAttribute('data-testid') === 'key-monitor');
  if (await ready()) return;
  await focusMonitor();
  await browser.waitUntil(ready, { timeout: 3_000, interval: 50, timeoutMsg: 'the window or the key monitor does not have keyboard focus' });
}

// The fake sink names keys as td-host does and the DOM as the browser does. Both become one vocabulary.
const NAMES: Record<string, string> = {
  Shift: 'Shift',
  LeftShift: 'Shift',
  Control: 'Ctrl',
  LeftCtrl: 'Ctrl',
  AudioVolumeUp: 'VolumeUp',
  AudioVolumeDown: 'VolumeDown',
};
const normal = (name: string) => NAMES[name] ?? name;

let fake = false;

/** Every key event the PC saw, as `down F13` or `up F13`, from the DOM or, on Linux, the fake sink. */
async function keyEvents(): Promise<string[]> {
  if (fake) {
    const log = (await invoke<string[] | null>('e2e_input_log')) ?? [];
    return log.map((line) => {
      const [type, name] = line.split(' ');
      return `${type} ${normal(name)}`;
    });
  }
  return $$('[data-testid="key-event"]').map(async (item) => `${await item.getAttribute('data-type')} ${normal((await item.getAttribute('data-key')) ?? '')}`);
}

async function reports(): Promise<string[]> {
  return $$('[data-testid="key-report"]').map((item) => item.getText());
}

async function eventsSince(start: number, want: number, timeout = 5_000) {
  let seen: string[] = [];
  await browser.waitUntil(
    async () => {
      seen = (await keyEvents()).slice(start);
      return seen.length >= want;
    },
    { timeout, interval: 50, timeoutMsg: `expected ${want} key events, saw ${JSON.stringify(seen)}` },
  );
  await browser.pause(150);
  return (await keyEvents()).slice(start);
}

async function focusMonitor() {
  const monitor = $('[data-testid="key-monitor"]');
  await browser.execute(() => (document.activeElement as HTMLElement | null)?.blur());
  await monitor.click();
  await browser.waitUntil(async () => (await monitor.getAttribute('data-front')) !== null, { timeout: 3_000, timeoutMsg: 'the monitor never asked for the foreground' });
  const front = (await monitor.getAttribute('data-front')) === 'true';
  if (!fake && !front) throw new Error('the window is not in the foreground, so typed keys would go to another window');
  return front;
}

async function clearMonitor() {
  await $('[data-testid="key-monitor-clear"]').click();
  await focusMonitor();
}

describe('key mirror on the mock deck', () => {
  it('connects and focuses the key monitor', async () => {
    await connectFirstDeck();
    fake = (await invoke<string[] | null>('e2e_input_log')) !== null;
    note('input', fake ? 'fake sink' : 'operating system');
    await $('[data-testid="view-actions"]').click();
    await browser.waitUntil(async () => (await $('[data-testid="mirror-state"]').getAttribute('data-enabled')) === 'true', {
      timeout: 3_000,
      timeoutMsg: 'mirroring is not on',
    });
    note('foreground', await focusMonitor());
  });

  it('types F13 and lists both reports', async () => {
    const start = (await keyEvents()).length;
    await emit([kbd(0, [F13]), kbd(0, [])]);
    const seen = await eventsSince(start, 2);
    expect(seen).toEqual(['down F13', 'up F13']);
    await browser.waitUntil(async () => (await reports()).length >= 2, { timeout: 3_000 });
    expect(await reports()).toEqual(['kbd 0 [104]', 'kbd 0 []']);
    const text = await $('[data-testid="key-monitor"]').getValue();
    if (!fake) expect(text).toBe('F13');
    note('monitor_text', text);
    await shot('f13');
  });

  it('presses Shift then F15 and releases them in reverse', async () => {
    await clearMonitor();
    const start = (await keyEvents()).length;
    await emit([kbd(SHIFT, [F15]), kbd(0, [])]);
    const seen = await eventsSince(start, 4);
    note('shift_f15', seen);
    expect(seen).toEqual(['down Shift', 'down F15', 'up F15', 'up Shift']);
  });

  it('taps volume up then volume down, which cancel', async () => {
    await clearMonitor();
    const start = (await keyEvents()).length;
    await emit([con(VOLUME_UP), con(0), con(VOLUME_DOWN), con(0)], 1, 60);
    await browser.waitUntil(async () => (await reports()).length >= 4, { timeout: 3_000 });
    expect(await reports()).toEqual(['con 233', 'con 0', 'con 234', 'con 0']);
    const seen = await eventsSince(start, 4).catch(async () => (await keyEvents()).slice(start));
    note('volume', seen);
    expect(seen).toEqual(['down VolumeUp', 'up VolumeUp', 'down VolumeDown', 'up VolumeDown']);
  });

  it('releases a key whose release was lost, then follows a consumer-only stream', async () => {
    await clearMonitor();
    const start = (await keyEvents()).length;
    await emit([kbd(0, [F16]), { m: 'drop', count: 1 }, con(VOLUME_UP), con(0), con(VOLUME_DOWN), con(0)], 1, 60);
    const seen = await eventsSince(start, 6);
    note('lost_release', seen);
    expect(seen).toEqual(['down F16', 'up F16', 'down VolumeUp', 'up VolumeUp', 'down VolumeDown', 'up VolumeDown']);
    expect((await reports()).some((line) => line.includes('lost') && line.includes('released all'))).toBe(true);
  });

  it('sets the held state exactly on key.resync', async () => {
    await clearMonitor();
    const start = (await keyEvents()).length;
    await emit([kbd(0, [F17]), { m: 'key.resync', mods: 0, keys: [] }], 1, 60);
    const seen = await eventsSince(start, 2);
    expect(seen).toEqual(['down F17', 'up F17']);
  });

  it('types a key within 30 ms of its report, at p95 over 100 taps', async () => {
    await clearMonitor();
    await emit([kbd(0, [F13]), kbd(0, [])], 100, 25);
    await browser.waitUntil(
      async () => (await keyEvents()).filter((line) => line === 'up F13').length >= 100,
      { timeout: 20_000, interval: 200, timeoutMsg: 'fewer than 100 F13 releases' },
    );
    if (fake) return;
    const latency = await browser.execute(() => {
      const arrived = [...document.querySelectorAll('[data-testid="key-report"]')]
        .filter((item) => item.textContent?.trim() === 'kbd 0 [104]')
        .map((item) => Number(item.getAttribute('data-arrived')));
      const downs = [...document.querySelectorAll('[data-testid="key-event"][data-type="down"][data-key="F13"]')].map((item) => Number(item.getAttribute('data-at')));
      // The report list keeps the last 50, 25 presses and their releases, so pair the presses with the last 25 key downs.
      const tail = downs.slice(-arrived.length);
      const gaps = arrived.map((at, i) => tail[i] - at).sort((a, b) => a - b);
      return { n: gaps.length, p50: gaps[Math.floor(gaps.length * 0.5)], p95: gaps[Math.floor(gaps.length * 0.95)], max: gaps[gaps.length - 1] };
    });
    note('key_latency_ms', latency);
    expect(latency.n).toBe(25);
    expect(latency.p95).toBeLessThan(30);
  });

  it('releases a held key when the session drops', async () => {
    await clearMonitor();
    const start = (await keyEvents()).length;
    await emit([kbd(0, [F18])]);
    await eventsSince(start, 1);
    await mockAction('hang-up');
    const seen = await eventsSince(start, 2);
    expect(seen).toEqual(['down F18', 'up F18']);
    await waitForDot('offline', 6_000);
  });

  it('types exactly 200 F14 taps from a burst and ends on a release', async () => {
    await $('[data-testid="view-console"]').click();
    await connectFirstDeck();
    await $('[data-testid="view-actions"]').click();
    await clearMonitor();
    const start = (await keyEvents()).length;
    await emit([kbd(0, [F14]), kbd(0, [])], 200, 25);
    await browser.waitUntil(
      async () => (await keyEvents()).slice(start).filter((line) => line === 'up F14').length >= 200,
      { timeout: 30_000, interval: 250, timeoutMsg: 'fewer than 200 F14 releases within 30 s' },
    );
    await mockAction('hang-up');
    await browser.pause(500);
    const seen = (await keyEvents()).slice(start);
    const downs = seen.filter((line) => line === 'down F14').length;
    note('burst', { downs, ups: seen.filter((line) => line === 'up F14').length, last: seen.at(-1) });
    expect(downs).toBe(200);
    expect(seen.at(-1)).toBe('up F14');
    if (!fake) {
      const text = await $('[data-testid="key-monitor"]').getValue();
      expect(text.split(' ').filter((word) => word === 'F14')).toHaveLength(200);
    }
    await shot('burst');
  });
});
