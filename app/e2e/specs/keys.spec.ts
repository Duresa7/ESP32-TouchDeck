import { execFileSync } from 'node:child_process';
import { connectFirstDeck, mockAction, waitForDot } from '../deck.ts';
import { note, shot } from '../report.ts';

// Only F13 to F24 and pairs of mute taps may reach the operating system. F13 opens Windows' Open with
// dialog on some PCs, so only the F13 test uses it and it closes that dialog afterwards.
const F13 = 104;
const F14 = 105;
const F15 = 106;
const F16 = 107;
const F17 = 108;
const F18 = 109;
const F20 = 111;
const MUTE = 226;

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

/** Whether the Open with dialog some PCs bind to F13 is open. It takes the foreground from this window. */
function openWithShowing(): boolean {
  if (process.platform !== 'win32') return false;
  try {
    return execFileSync('tasklist', ['/FI', 'IMAGENAME eq OpenWith.exe', '/NH'], { encoding: 'utf8' }).includes('OpenWith.exe');
  } catch {
    return false;
  }
}

function dismissOpenWith() {
  if (!openWithShowing()) return;
  try {
    execFileSync('taskkill', ['/F', '/IM', 'OpenWith.exe'], { stdio: 'ignore' });
  } catch {
    // It closed by itself.
  }
}

/** Keys go to the foreground window, so make sure that is this window with the monitor focused. */
async function holdFocus() {
  const ready = () =>
    browser.execute(() => document.hasFocus() && document.activeElement?.getAttribute('data-testid') === 'key-monitor');
  // The app reports whether the operating system has this window in front, which hasFocus alone does not.
  await browser.waitUntil(async () => {
    const front = await invoke<boolean>('focus_window');
    await browser.execute(() => (document.querySelector('[data-testid="key-monitor"]') as HTMLElement)?.focus());
    return front && (await ready());
  }, { timeout: 5_000, interval: 100, timeoutMsg: 'the window is not in front, or the key monitor does not have keyboard focus' });
}
// The fake sink names keys as td-host does and the DOM as the browser does. Both become one vocabulary.
const NAMES: Record<string, string> = {
  Shift: 'Shift',
  LeftShift: 'Shift',
  Control: 'Ctrl',
  LeftCtrl: 'Ctrl',
  AudioVolumeMute: 'Mute',
  VolumeMute: 'Mute',
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
  const events = await browser.execute(() => Array.from(document.querySelectorAll('[data-testid="key-event"]'), (item) => ({
    type: item.getAttribute('data-type'), key: item.getAttribute('data-key') ?? '',
  })));
  return events.map((event) => `${event.type} ${normal(event.key)}`);
}

async function reports(): Promise<string[]> {
  return browser.execute(() => Array.from(document.querySelectorAll('[data-testid="key-report"]'), (item) => item.textContent?.trim() ?? ''));
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
  await browser.execute(() => (document.querySelector('[data-testid="key-monitor"]') as HTMLElement)?.focus());
  if (!fake) await holdFocus();
  return browser.execute(() => document.hasFocus());
}

async function clearMonitor() {
  await $('[data-testid="key-monitor-clear"]').click();
  await focusMonitor();
}

describe('key mirror on the mock deck', () => {
  before(async () => {
    await browser.setTimeout({ script: 5_000, implicit: 1_000, pageLoad: 5_000 });
    await browser.execute(() => {
      document.documentElement.dataset.focusLosses = '0';
      window.addEventListener('blur', () => {
        document.documentElement.dataset.focusLosses = String(Number(document.documentElement.dataset.focusLosses) + 1);
      });
    });
  });

  beforeEach(function () {
    console.log(`keys: ${this.currentTest?.title}`);
  });

  afterEach(async function () {
    dismissOpenWith();
    if (this.currentTest?.state !== 'failed') return;
    note('failure', await browser.execute(() => ({
      focus: document.hasFocus(), active: document.activeElement?.getAttribute('data-testid'),
      focusLosses: document.documentElement.dataset.focusLosses,
      events: Array.from(document.querySelectorAll('[data-testid="key-event"]'), (item) => item.textContent),
      reports: Array.from(document.querySelectorAll('[data-testid="key-report"]'), (item) => item.textContent),
    })));
    await shot('failure');
  });

  it('connects and focuses the key monitor', async () => {
    await connectFirstDeck();
    fake = (await invoke<string[] | null>('e2e_input_log')) !== null;
    note('input', fake ? 'fake sink' : 'operating system');
    await $('[data-testid="nav-actions"]').click();
    await browser.waitUntil(async () => (await invoke<{ enabled: boolean }>('mirror_state')).enabled, {
      timeout: 3_000,
      timeoutMsg: 'mirroring is not on',
    });
    note('foreground', await focusMonitor());
  });

  it('types F13 and lists both reports', async () => {
    dismissOpenWith();
    await focusMonitor();
    const start = (await keyEvents()).length;
    await emit([kbd(0, [F13]), kbd(0, [])]);
    await browser.waitUntil(async () => (await reports()).length >= 2, { timeout: 3_000 });
    expect(await reports()).toEqual(['kbd 0 [104]', 'kbd 0 []']);
    // On a PC where F13 is a global hotkey, the OS takes the key before the webview sees it. Say so
    // and do not read the key events, since the dialog it opened now holds the foreground.
    let seen: string[] = [];
    await browser.waitUntil(async () => {
      seen = (await keyEvents()).slice(start);
      return seen.length >= 2 || openWithShowing();
    }, { timeout: 5_000, interval: 50, timeoutMsg: `expected 2 key events, saw ${JSON.stringify(seen)}` });
    const intercepted = seen.length < 2 && openWithShowing();
    note('f13_intercepted_by_os', intercepted);
    if (!intercepted) {
      await browser.pause(150);
      expect((await keyEvents()).slice(start)).toEqual(['down F13', 'up F13']);
      const text = await $('[data-testid="key-monitor"]').getValue();
      if (!fake) expect(text).toBe('F13');
      note('monitor_text', text);
    }
    await shot('f13');
    dismissOpenWith();
    await focusMonitor();
  });
  it('presses F14 then F15 and releases them in reverse', async () => {
    await clearMonitor();
    const start = (await keyEvents()).length;
    await emit([kbd(0, [F14, F15]), kbd(0, [])]);
    const seen = await eventsSince(start, 4);
    note('f14_f15', seen);
    expect(seen).toEqual(['down F14', 'down F15', 'up F15', 'up F14']);
  });

  it('taps mute twice to restore the initial audio state', async () => {
    await clearMonitor();
    const start = (await keyEvents()).length;
    await emit([con(MUTE), con(0), con(MUTE), con(0)], 1, 60);
    await browser.waitUntil(async () => (await reports()).length >= 4, { timeout: 3_000 });
    expect(await reports()).toEqual(['con 226', 'con 0', 'con 226', 'con 0']);
    const seen = await eventsSince(start, 4).catch(async () => (await keyEvents()).slice(start));
    note('mute_pair', seen);
    expect(seen).toEqual(['down Mute', 'up Mute', 'down Mute', 'up Mute']);
  });

  it('releases a key whose release was lost, then follows a consumer-only stream', async () => {
    await clearMonitor();
    const start = (await keyEvents()).length;
    await emit([kbd(0, [F16]), { m: 'drop', count: 1 }, con(MUTE), con(0), con(MUTE), con(0)], 1, 60);
    const seen = await eventsSince(start, 6);
    note('lost_release', seen);
    expect(seen).toEqual(['down F16', 'up F16', 'down Mute', 'up Mute', 'down Mute', 'up Mute']);
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
    await emit([kbd(0, [F20]), kbd(0, [])], 100, 25);
    await browser.waitUntil(
      async () => (await keyEvents()).filter((line) => line === 'up F20').length >= 100,
      { timeout: 20_000, interval: 200, timeoutMsg: 'fewer than 100 F20 releases' },
    );
    if (fake) return;
    const latency = await browser.execute(() => {
      const arrived = [...document.querySelectorAll('[data-testid="key-report"]')]
        .filter((item) => item.textContent?.trim() === 'kbd 0 [111]')
        .map((item) => Number(item.getAttribute('data-arrived')));
      const downs = [...document.querySelectorAll('[data-testid="key-event"][data-type="down"][data-key="F20"]')].map((item) => Number(item.getAttribute('data-at')));
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
    await $('[data-testid="nav-deck"]').click();
    await connectFirstDeck();
    await $('[data-testid="nav-actions"]').click();
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
