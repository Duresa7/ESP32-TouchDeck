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

  it('shows only Wi-Fi setup and keeps the password out of the console', async () => {
    expect(await browser.execute(() => /bluetooth/i.test(document.body.textContent ?? ''))).toBe(false);
    expect(await $('[data-testid="ble-pair"]').isExisting()).toBe(false);
    await $('[data-testid="nav-actions"]').click();
    expect(await $('[data-testid="mirror-state"]').isExisting()).toBe(false);
    await shot('actions');
    await $('[data-testid="nav-deck"]').click();
    note('password_in_console', await browser.execute(() => document.body.textContent?.includes('mock-only-pass') ?? false));
    expect(await browser.execute(() => document.body.textContent?.includes('mock-only-pass') ?? false)).toBe(false);
  });

});
