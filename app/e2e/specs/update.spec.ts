import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { connectFirstDeck } from '../deck.ts';
import { note, out, shot } from '../report.ts';

async function selectBundle(name: string) {
  const bytes = [...readFileSync(fileURLToPath(new URL(`../../crates/td-link/tests/fixtures/${name}`, import.meta.url)))];
  await browser.execute((data, filename) => {
    const input = document.querySelector('[data-testid="update-file"]');
    if (!(input instanceof HTMLInputElement)) throw new Error('firmware input missing');
    const transfer = new DataTransfer();
    transfer.items.add(new File([new Uint8Array(data)], filename));
    input.files = transfer.files;
    input.dispatchEvent(new Event('change', { bubbles: true }));
  }, bytes, name);
}
async function mockMode(action: string) {
  await browser.execute(async (action) => {
    if (!('__TAURI__' in window)) throw new Error('Tauri missing');
    const api = window.__TAURI__;
    if (!api || typeof api !== 'object' || !('core' in api)) throw new Error('Tauri core missing');
    const core = api.core;
    if (!core || typeof core !== 'object' || !('invoke' in core) || typeof core.invoke !== 'function') throw new Error('invoke missing');
    await core.invoke('e2e_mock', { action });
  }, action);
}
const status = () => $('[data-testid="update-status"]').getText();
const finished = async (text: string, timeout = 10_000) => browser.waitUntil(async () => (await status()).includes(text), { timeout, timeoutMsg: `update did not show ${text}` });

describe('firmware update on the mock deck', () => {
  it('refuses an incomplete bundle before transfer', async () => {
    await connectFirstDeck();
    await $('[data-testid="nav-update"]').click();
    await selectBundle('touchdeck-test-missing.tdfw');
    await $('[data-testid="update-start"]').click();
    await finished('Failed');
    await shot('bad-bundle');
  });
  it('sends the image, reboots, and confirms the expected build', async () => {
    await selectBundle('touchdeck-test.tdfw');
    await $('[data-testid="update-start"]').click();
    await finished('Firmware updated');
    const history = await $('[data-testid="update-history"]').getText();
    for (const state of ['Checking', 'Sending 100%', 'Rebooting', 'Confirming', 'Firmware updated']) expect(history).toContain(state);
    writeFileSync(join(out, 'states.json'), JSON.stringify({ history }, null, 2));
    note('states', history);
    await shot('mock-update');
  });
  it('skips the current build', async () => {
    await $('[data-testid="update-start"]').click();
    await finished('Already on this build');
    expect(await $('[data-testid="update-history"]').getText()).not.toContain('Sending');
    await shot('same');
  });
  it('reports a rollback and keeps recovery available', async () => {
    await mockMode('rollback');
    await selectBundle('touchdeck-test-b.tdfw');
    await $('[data-testid="update-start"]').click();
    await finished('rolled back');
    expect(await $('[data-testid="update-recover"]').isEnabled()).toBe(true);
    await shot('mock-rollback');
  });
  it('reports confirmation timeout after 30 seconds and offers recovery', async () => {
    await mockMode('no-confirm');
    await $('[data-testid="update-start"]').click();
    await finished('confirm timeout', 40_000);
    expect(await $('[data-testid="update-recover"]').isEnabled()).toBe(true);
    await shot('no-confirm');
  });
});
