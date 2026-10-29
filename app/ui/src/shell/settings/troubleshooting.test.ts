import { describe, expect, test } from 'vitest';
import type { LinkState } from '../../companion/api.ts';
import { sampleDevice } from '../../companion/samples.ts';
import { createUiStore } from '../../state/ui.ts';
import { connectionText, factsOf, lastError } from './troubleshooting.ts';

const connected: LinkState = { state: 'connected', port: 'COM7', since_ms: 0, protocol: 2 };

describe('Troubleshooting', () => {
  test('a connected Device shows the eight fields under the copy labels', () => {
    const device = sampleDevice('a0b1', 'Desk', connected);
    device.layout = { name: 'Streaming', pages: 3 };
    expect(factsOf(device).map((f) => [f.label, f.value])).toEqual([
      ['Product', null],
      ['Protocol', '2'],
      ['Firmware build', 'fake'],
      ['Firmware version', '1.4.0'],
      ['Running from', null],
      ['Sends keys through', null],
      ['Safe mode', 'No'],
      ['Current Layout', 'Streaming'],
    ]);
  });

  test('without Firmware details or a Layout summary nothing is invented', () => {
    const device = { ...sampleDevice('a0b1', 'Desk', connected), firmware: null, layout: ['not', 'a', 'summary'] };
    expect(factsOf(device).every((f) => f.value === null)).toBe(true);
  });

  test.each<[LinkState, string]>([
    [connected, 'Desk is on COM7.'],
    [{ state: 'connecting', port: 'COM7' }, 'Connecting…'],
    [{ state: 'busy', port: 'COM7', detail: 'Access is denied.' }, 'Another program is using Desk. Close it, and Desk reconnects on its own.'],
    [{ state: 'unresponsive', port: 'COM7' }, "Desk didn't answer. Unplug it and plug it back in."],
    [{ state: 'lost', reason: 'timeout', detail: 'no reply within 1500 ms', at_ms: 1 }, 'Lost Desk. Check the USB cable. It reconnects on its own.'],
    [{ state: 'updating', job: 'job_1' }, 'Desk is updating. Wait for it to finish.'],
    [{ state: 'offline' }, "Desk isn't connected."],
    [{ state: 'disconnected' }, "Desk isn't connected."],
  ])('says where the Device is or what is wrong: %o', (link, text) => {
    expect(connectionText({ device: sampleDevice('a0b1', 'Desk', link), demo: false })).toBe(text);
  });

  test('the mock Device is the demo Device, and an unnamed one still reads as a sentence', () => {
    expect(connectionText({ device: sampleDevice('mock0001', 'Desk', connected), demo: true })).toBe('Desk is the demo Device.');
    expect(connectionText({ device: sampleDevice('a0b1', null, { state: 'offline' }), demo: false })).toBe("Unnamed Device isn't connected.");
  });

  test('the raw link error is kept for Details only when there is one', () => {
    expect(lastError(sampleDevice('a', 'Desk', { state: 'lost', reason: 'io-error', detail: 'write failed: 31', at_ms: 1 }))).toBe('write failed: 31');
    expect(lastError(sampleDevice('a', 'Desk', { state: 'lost', reason: 'unplugged', detail: '', at_ms: 1 }))).toBeNull();
    expect(lastError(sampleDevice('a', 'Desk', connected))).toBeNull();
  });

  test('Settings opened from the nav starts on its main view, even after Connection help opened Troubleshooting', () => {
    const ui = createUiStore();
    ui.getState().openTroubleshooting();
    expect(ui.getState()).toMatchObject({ settingsOpen: true, settingsSection: 'troubleshooting' });
    ui.getState().setSettingsOpen(false);
    expect(ui.getState().settingsSection).toBe('troubleshooting');
    ui.getState().setSettingsOpen(true);
    expect(ui.getState()).toMatchObject({ settingsOpen: true, settingsSection: 'general' });
    ui.getState().showSettingsSection('troubleshooting');
    expect(ui.getState().settingsSection).toBe('troubleshooting');
  });
});
