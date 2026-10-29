import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import type { LinkState } from '../../companion/api.ts';
import { createClient } from '../../companion/client.ts';
import { consoleCommands } from '../../companion/console.ts';
import { createFakeCompanion, type FakeCompanion } from '../../companion/fake.ts';
import { isAllowedConsoleLine } from '../../companion/fake-console.ts';
import { sampleDevice } from '../../companion/samples.ts';
import { windowSession } from '../../companion/session.ts';
import { CONSOLE_KEEPS, createConsoleStore, type ConsoleStore } from './console.ts';

const BASE = 'http://127.0.0.1:51234';
const connected: LinkState = { state: 'connected', port: 'COM3', since_ms: 0, protocol: 2 };

let fake: FakeCompanion;
let desk: ConsoleStore;

beforeEach(() => {
  fake = createFakeCompanion({ devices: [sampleDevice('mock0001', 'Desk', connected), sampleDevice('mock0002', 'Studio')] });
  vi.stubGlobal('fetch', fake.fetch);
  desk = createConsoleStore({ companion: createClient(BASE, windowSession(fake.bridge)), device: 'mock0001', name: 'Desk' });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('the Firmware console', () => {
  test('an allowed line goes to the Device and shows as sent', async () => {
    expect(await desk.getState().send('  layout state ')).toBe('sent');
    expect(fake.consoles.sent).toEqual([{ device: 'mock0001', line: 'layout state' }]);
    expect(desk.getState().entries).toEqual([{ kind: 'sent', text: 'layout state' }]);
    expect(desk.getState().notice).toBeNull();
  });

  test('a forbidden line is not kept as sent, keeps the field, and names the line for the allowlist', async () => {
    expect(await desk.getState().send('reboot')).toBe('kept');
    expect(fake.consoles.sent).toEqual([]);
    expect(desk.getState().entries).toEqual([]);
    expect(desk.getState().notice).toEqual({ kind: 'forbidden', line: 'reboot' });

    expect(await desk.getState().send('version')).toBe('sent');
    expect(desk.getState().notice).toBeNull();
    expect(desk.getState().entries).toEqual([{ kind: 'sent', text: 'version' }]);
  });

  test('every refusal reason in the contract is forbidden, and every listed form passes', async () => {
    for (const line of ['keys a', 'store put /x', 'version extra', 'store get /a;reboot', 'store get $HOME', 'link  state', 'VERSION', 'ui tile café', `store get /${'a'.repeat(250)}`]) {
      expect(await desk.getState().send(line), line).toBe('kept');
      expect(desk.getState().notice, line).toEqual({ kind: 'forbidden', line: line.trim() });
    }
    for (const form of consoleCommands) expect(isAllowedConsoleLine(form.replace(/<\w+>/, 'home')), form).toBe(true);
  });

  test('a Device that is not connected gets the copy sentence, not the raw error', async () => {
    const studio = createConsoleStore({ companion: createClient(BASE, windowSession(fake.bridge)), device: 'mock0002', name: 'Studio' });
    expect(await studio.getState().send('version')).toBe('kept');
    expect(studio.getState().notice).toEqual({ kind: 'failed', text: 'Plug in Studio to use the console.' });
  });

  test('a blank line sends nothing', async () => {
    expect(await desk.getState().send('   ')).toBe('kept');
    expect(fake.requests).toBe(0);
  });

  test('received lines append, and the log keeps the last 500', () => {
    for (let i = 0; i < CONSOLE_KEEPS + 3; i++) desk.getState().receive(`line ${i}`);
    const { entries } = desk.getState();
    expect(entries).toHaveLength(CONSOLE_KEEPS);
    expect(entries[0]).toEqual({ kind: 'line', text: 'line 3' });
  });
});
