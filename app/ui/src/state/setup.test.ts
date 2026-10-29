import { describe, expect, test } from 'vitest';
import { readSkipped, SKIPPED_KEY, type SkipStorage } from '../lib/device/setup-trigger.ts';
import { createSetupUi } from './setup.ts';

function memoryStorage(): SkipStorage {
  const items = new Map<string, string>();
  return { getItem: (key) => items.get(key) ?? null, setItem: (key, value) => void items.set(key, value) };
}

describe('the setup dialog', () => {
  test('closing before the Device is set up remembers the skip, here and next time', () => {
    const storage = memoryStorage();
    const ui = createSetupUi(storage);
    ui.getState().start('a');
    expect(ui.getState().open).toBe('a');
    ui.getState().close({ id: 'a', setup: 'needs-setup' });
    expect(ui.getState().open).toBeNull();
    expect(ui.getState().skipped).toEqual(new Set(['a']));
    expect(createSetupUi(storage).getState().skipped).toEqual(new Set(['a']));
  });

  test('closing after setup finished skips nothing', () => {
    const storage = memoryStorage();
    const ui = createSetupUi(storage);
    ui.getState().start('a');
    ui.getState().close({ id: 'a', setup: 'ready' });
    expect(ui.getState().skipped).toEqual(new Set());
    expect(storage.getItem(SKIPPED_KEY)).toBeNull();
  });

  test('closing for a Device that went away only closes', () => {
    const ui = createSetupUi(memoryStorage());
    ui.getState().start('a');
    ui.getState().close();
    expect(ui.getState()).toMatchObject({ open: null, skipped: new Set() });
  });

  test('a fresh install lets a skipped Device set up again', () => {
    const storage = memoryStorage();
    const ui = createSetupUi(storage);
    ui.getState().close({ id: 'a', setup: 'needs-setup' });
    ui.getState().unskip('a');
    expect(ui.getState().skipped).toEqual(new Set());
    expect(readSkipped(storage)).toEqual(new Set());
  });

  test('without storage the skip lasts until the app closes', () => {
    const ui = createSetupUi(null);
    ui.getState().close({ id: 'a', setup: 'needs-setup' });
    expect(ui.getState().skipped).toEqual(new Set(['a']));
  });
});
