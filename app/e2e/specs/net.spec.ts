import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { HELLO_FIELDS, MOCK_ELF_SHA256, row, waitForDot } from '../deck.ts';
import { note, shot } from '../report.ts';

const node = process.execPath;
const scratch = join(tmpdir(), `td-net-${process.pid}-${Date.now()}`);
const launched = join(scratch, 'net-launched.txt');

interface MockRecord {
  received: { m: string }[];
  layouts: { landscape: { name: string } | null; portrait: { name: string } | null };
  net: { port: number; code: string; paired: number };
}

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

describe('Wi-Fi discovery, PAKE pairing, and encrypted link session', () => {
  let netPort = '';

  before(() => {
    mkdirSync(scratch, { recursive: true });
  });

  after(() => {
    rmSync(scratch, { recursive: true, force: true });
  });

  it('discovers the Wi-Fi deck as unpaired and accepts manual IP entry', async () => {
    const rec = await invoke<MockRecord>('e2e_mock_record');
    netPort = `net:127.0.0.1:${rec.net.port}`;
    await row(netPort).waitForExist({ timeout: 10_000 });
    expect(await row(netPort).getAttribute('data-status')).toBe('net-unpaired');

    // Manual IP entry for 127.0.0.1 resolves to the mock Wi-Fi port
    await $('[data-testid="net-ip-input"]').setValue('127.0.0.1');
    await $('[data-testid="net-ip-add"]').click();
    expect(await row(netPort).getAttribute('data-status')).toBe('net-unpaired');
    note('discovered_port', netPort);
    await shot('net-discovered');
  });

  it('refuses a wrong pairing code without storing a credential', async () => {
    await row(netPort).$('[data-testid="pair"]').click();
    const modal = $('[data-testid="pair-modal"]');
    await modal.waitForExist({ timeout: 5_000 });

    await $('[data-testid="pair-code-input"]').setValue('000000');
    await $('[data-testid="pair-submit"]').click();

    const error = $('[data-testid="pair-error"]');
    await error.waitForExist({ timeout: 5_000 });
    expect(await error.getText()).toMatch(/Wrong code/);

    const rec = await invoke<MockRecord>('e2e_mock_record');
    expect(rec.net.paired).toBe(0);
    await shot('net-wrong-code');
  });

  it('pairs with the deck code and connects over encrypted net-v1', async () => {
    const rec = await invoke<MockRecord>('e2e_mock_record');
    await $('[data-testid="pair-code-input"]').setValue(rec.net.code);
    await $('[data-testid="pair-submit"]').click();

    await $('[data-testid="pair-modal"]').waitForExist({ timeout: 5_000, reverse: true });
    await waitForDot('online', 5_000);

    expect(await row(netPort).getAttribute('data-status')).toBe('net-paired');
    for (const field of HELLO_FIELDS) {
      expect(await $(`[data-testid="hello-${field}"] dd`).getText()).not.toBe('');
    }
    expect(await $('[data-testid="hello-elf_sha256"] dd').getText()).toBe(MOCK_ELF_SHA256);

    const afterRec = await invoke<MockRecord>('e2e_mock_record');
    expect(afterRec.net.paired).toBe(1);
    note('paired_port', netPort);
    await shot('net-paired-connected');
  });

  it('pushes a layout from the Editor tab over Wi-Fi', async () => {
    await $('[data-testid="nav-editor"]').click();
    await $('[data-testid="new-layout"]').waitForExist({ timeout: 5_000 });
    await $('[data-testid="new-layout"]').click();
    await $('[data-testid="add-tile"]').click();
    await $('[data-testid="push"]').click();

    await browser.waitUntil(
      async () => /^Deck shows /.test(await $('[data-testid="editor-status"]').getText()),
      { timeout: 5_000, interval: 50, timeoutMsg: 'editor layout push over Wi-Fi did not finish' },
    );

    const rec = await invoke<MockRecord>('e2e_mock_record');
    expect(rec.layouts.landscape).not.toBeNull();
    note('pushed_layout_over_wifi', rec.layouts.landscape?.name ?? null);
    await shot('net-editor-pushed');
  });

  it('receives a press event over Wi-Fi and triggers the tile action', async () => {
    const layout = JSON.parse(readFileSync(join(import.meta.dirname, '..', '..', '..', 'schema', 'examples', 'dev.json'), 'utf8'));
    const desktop = layout.pages.find((page: { id: string }) => page.id === 'desktop');
    const launchTile = desktop.tiles.find((t: { id: string }) => t.id === 'launch');
    launchTile.action = {
      type: 'launch',
      path: node,
      args: ['-e', `require('fs').writeFileSync(${JSON.stringify(launched)}, 'wifi-press-ok')`],
      cwd: '',
    };
    await invoke('set_action_layout', { json: JSON.stringify(layout), source: 'net.spec' });

    await $('[data-testid="nav-actions"]').click();
    await invoke('e2e_emit', { events: [{ m: 'press', page: 'desktop', tile: 'launch', kind: 'tap' }] });

    await browser.waitUntil(() => existsSync(launched), {
      timeout: 5_000,
      interval: 50,
      timeoutMsg: 'press event over Wi-Fi did not run launch action',
    });
    expect(readFileSync(launched, 'utf8')).toBe('wifi-press-ok');
    await shot('net-press-event');
  });

  it('reconnects over Wi-Fi after disconnect without pairing again', async () => {
    await $('[data-testid="nav-deck"]').click();
    await row(netPort).$('[data-testid="disconnect"]').click();
    await waitForDot('offline', 5_000);

    // Still paired; Connect reconnects directly without opening the pairing modal
    expect(await row(netPort).getAttribute('data-status')).toBe('net-paired');
    await row(netPort).$('[data-testid="connect"]').click();
    await waitForDot('online', 5_000);
    expect(await $('[data-testid="pair-modal"]').isExisting()).toBe(false);
    await shot('net-reconnected');
  });
});
