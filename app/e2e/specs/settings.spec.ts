import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { connectFirstDeck } from '../deck.ts';
import { note, out, shot } from '../report.ts';

interface Record {
  received: { m: string; idle_timeout_s?: number; orientation?: string }[];
  settings: { idle_timeout_s: number; orientation: string };
  lines: string[];
  reboots: number;
}

const record = () =>
  browser.execute(async () => {
    const tauri = (window as unknown as { __TAURI__: { core: { invoke: (cmd: string) => Promise<unknown> } } }).__TAURI__;
    return tauri.core.invoke('e2e_mock_record');
  }) as Promise<Record>;

const statusText = () => $('[data-testid="settings-status"]').getText();

async function setField(testid: string, value: string) {
  await browser.execute(
    (id, next) => {
      const field = document.querySelector<HTMLInputElement | HTMLSelectElement>(`[data-testid="${id}"]`)!;
      field.value = next;
      field.dispatchEvent(new Event(field instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true }));
    },
    testid,
    value,
  );
}

describe('deck settings on the mock deck', () => {
  it('saves 120 seconds and portrait and offers a reboot', async () => {
    note('port', await connectFirstDeck());
    await $('[data-testid="nav-settings"]').click();
    await browser.waitUntil(async () => /current settings/.test(await statusText()), { timeout: 5_000, timeoutMsg: 'the page never read the settings' });
    expect(await $('[data-testid="settings-idle"]').getValue()).toBe('600');
    expect(await $('[data-testid="wireless"]').isExisting()).toBe(false);
    const initial = await record();
    expect(initial.received.some(({ m }) => /^(wifi|ble)\./.test(m))).toBe(false);
    note('usb_settings', { wireless_controls: false, unsupported_requests: 0 });
    await setField('settings-idle', '120');
    await setField('settings-orientation', 'portrait');
    await $('[data-testid="settings-save"]').click();
    await browser.waitUntil(async () => /^Saved/.test(await statusText()), { timeout: 5_000, timeoutMsg: 'Save never answered' });
    await $('[data-testid="settings-reboot"]').waitForExist({ timeout: 2_000 });
    const saved = await record();
    const put = saved.received.filter((m) => m.m === 'settings.put').at(-1);
    expect(put).toEqual({ m: 'settings.put', idle_timeout_s: 120, orientation: 'portrait' });
    expect(saved.settings).toEqual({ idle_timeout_s: 120, orientation: 'portrait' });
    mkdirSync(out, { recursive: true });
    writeFileSync(join(out, 'settings.json'), JSON.stringify({ put, settings: saved.settings, status: await statusText() }, null, 2) + '\n');
    await shot('settings');
  });

  it('refuses an idle timeout outside 1 to 86400 seconds', async () => {
    await setField('settings-idle', '86401');
    await $('[data-testid="settings-idle-error"]').waitForExist({ timeout: 2_000 });
    expect(await $('[data-testid="settings-save"]').isEnabled()).toBe(false);
    await setField('settings-idle', '120');
  });

  it('reboots the deck through its console', async () => {
    await $('[data-testid="settings-reboot"]').click();
    await browser.waitUntil(async () => /restarting/.test(await statusText()), { timeout: 5_000, timeoutMsg: 'Reboot never answered' });
    const after = await record();
    expect(after.reboots).toBe(1);
    expect(after.lines.at(-1)).toBe('reboot');
    note('reboot', { reboots: after.reboots, status: await statusText() });
  });
});
