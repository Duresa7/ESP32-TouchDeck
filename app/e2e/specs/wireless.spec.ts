import { connectFirstDeck } from '../deck.ts';
import { note, shot } from '../report.ts';

function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.execute(async (name, payload) => {
    const tauri = (window as unknown as { __TAURI__: { core: { invoke: (cmd: string, args: object) => Promise<unknown> } } }).__TAURI__;
    return tauri.core.invoke(name, payload);
  }, command, args) as Promise<T>;
}

async function field(id: string, value: string) {
  await browser.execute((testid, next) => {
    const input = document.querySelector<HTMLInputElement>(`[data-testid="${testid}"]`)!;
    input.value = next;
    input.dispatchEvent(new Event(input instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true }));
  }, id, value);
}

const inputLog = () => invoke<string[]>('e2e_input_log');
const key = (keys: number[]) => ({ m: 'key.report', type: 'kbd', mods: 0, keys });
const emit = (events: object[]) => invoke<void>('e2e_emit', { events, everyMs: 30 });
const mirror = () => invoke<{ enabled: boolean; reason: string }>('mirror_state');

async function wifiState(state: string) {
  await browser.waitUntil(async () => (await $('[data-testid="wifi-state"]').getAttribute('data-state')) === state, { timeout: 7000 });
}

describe('wireless setup on the mock deck', () => {
  it('joins a scanned network, clears the password, and shows an IP', async () => {
    await connectFirstDeck();
    expect(await inputLog()).not.toBeNull();
    note('input', 'fake sink');
    await $('[data-testid="nav-settings"]').click();
    await wifiState('off');
    await $('[data-testid="wifi-scan"]').click();
    await $('[data-testid="wifi-networks"]').waitForExist();
    await field('wifi-networks', 'Mock home');
    expect(await $('[data-testid="wifi-password"]').getAttribute('type')).toBe('password');
    await field('wifi-password', 'mock-only-pass');
    await $('[data-testid="wifi-join"]').click();
    await wifiState('connecting');
    await wifiState('connected');
    expect(await $('[data-testid="wifi-state"]').getText()).toContain('192.168.50.42');
    expect(await $('[data-testid="wifi-password"]').getValue()).toBe('');
    const record = await invoke<{ received: Record<string, unknown>[]; lines: string[] }>('e2e_mock_record');
    expect(record.received.every((request) => !('password' in request))).toBe(true);
    expect(JSON.stringify(record).includes('mock-only-pass')).toBe(false);
    note('wifi_connected', await $('[data-testid="wifi-state"]').getText());
    note('password_in_mock_record', false);
    await shot('wifi-connected');
    await $('[data-testid="wifi-forget"]').click();
    await wifiState('off');
  });

  it('shows a failed join and can forget it', async () => {
    await field('wifi-networks', 'Mock rejected');
    await field('wifi-password', 'mock-only-pass');
    await $('[data-testid="wifi-join"]').click();
    await wifiState('failed');
    expect(await $('[data-testid="wifi-state"]').getText()).toContain('auth');
    expect(await $('[data-testid="wifi-password"]').getValue()).toBe('');
    note('wifi_failed', await $('[data-testid="wifi-state"]').getText());
    await shot('wifi-failed');
    await $('[data-testid="wifi-forget"]').click();
    await wifiState('off');
  });

  it('releases held keys, pauses for Bluetooth, and resumes after forgetting', async () => {
    await emit([key([105, 106])]);
    await browser.waitUntil(async () => (await inputLog()).length === 2);
    expect(await inputLog()).toEqual(['down F14', 'down F15']);
    await $('[data-testid="ble-pair"]').click();
    await browser.waitUntil(async () => (await $('[data-testid="ble-state"]').getAttribute('data-connected')) === 'true');
    expect(await $('[data-testid="ble-state"]').getText()).toContain('1 bonded');
    await browser.waitUntil(async () => !(await mirror()).enabled);
    expect(await inputLog()).toEqual(['down F14', 'down F15', 'up F15', 'up F14']);
    note('ble_connected', await $('[data-testid="ble-state"]').getText());
    await shot('bluetooth-connected');
    await $('[data-testid="nav-actions"]').click();
    await browser.waitUntil(async () => (await $('[data-testid="mirror-state"]').getText()).includes('Bluetooth'));
    await emit([key([107]), { m: 'key.resync', mods: 0, keys: [108] }, { m: 'key.report', type: 'con', usage: 226 }]);
    await browser.waitUntil(async () => (await $$('[data-testid="key-report"][data-outcome="off"]')).length === 3);
    expect(await inputLog()).toEqual(['down F14', 'down F15', 'up F15', 'up F14']);
    note('paused_reports', 3);
    await shot('bluetooth-paused');
    await $('[data-testid="nav-settings"]').click();
    await browser.waitUntil(async () => (await $('[data-testid="ble-state"]').getText()).includes('1 bonded'));
    await $('[data-testid="ble-forget"]').click();
    await browser.waitUntil(async () => (await mirror()).enabled);
    await browser.waitUntil(async () => (await $('[data-testid="ble-state"]').getText()).includes('0 bonded'));
    await emit([key([109]), key([])]);
    await browser.waitUntil(async () => (await inputLog()).length === 6);
    expect(await inputLog()).toEqual(['down F14', 'down F15', 'up F15', 'up F14', 'down F18', 'up F18']);
    note('pause_resume_input', await inputLog());
  });

  it('switches Bluetooth off and back on and shows the app typing path', async () => {
    await $('[data-testid="ble-enabled"]').click();
    await browser.waitUntil(async () => !(await $('[data-testid="ble-enabled"]').isSelected()));
    expect(await $('[data-testid="ble-pair"]').isEnabled()).toBe(false);
    await $('[data-testid="ble-enabled"]').click();
    await browser.waitUntil(async () => (await $('[data-testid="ble-enabled"]').isSelected()));
    await $('[data-testid="nav-actions"]').click();
    await browser.waitUntil(async () => (await $('[data-testid="mirror-state"]').getText()).includes('mirrored USB'));
    await shot('usb-mirroring-resumed');
    await $('[data-testid="nav-deck"]').click();
    note('password_in_console', await browser.execute(() => document.body.textContent?.includes('mock-only-pass') ?? false));
    expect(await browser.execute(() => document.body.textContent?.includes('mock-only-pass') ?? false)).toBe(false);
  });
});
