import { describe, expect, test } from 'vitest';
import type { DeviceSummary, LinkState } from '../../companion/api.ts';
import { sampleDevice } from '../../companion/samples.ts';
import { readSkipped, setupCandidate, SKIPPED_KEY, writeSkipped, type SkipStorage } from './setup-trigger.ts';

const connected = { state: 'connected', port: 'COM3', since_ms: 0, protocol: 2 } satisfies LinkState;
const unnamed = (id: string, link: DeviceSummary['link'] = connected) => sampleDevice(id, null, link);

function memoryStorage(seed: string | null = null): SkipStorage & { value: string | null } {
  const storage = {
    value: seed,
    getItem: (key: string) => (key === SKIPPED_KEY ? storage.value : null),
    setItem: (key: string, value: string) => {
      if (key === SKIPPED_KEY) storage.value = value;
    },
  };
  return storage;
}

describe('which Device sets up', () => {
  test('a connected Device that needs setup is the one', () => {
    expect(setupCandidate([sampleDevice('a', 'Desk', connected), unnamed('b')], new Set())?.id).toBe('b');
  });

  test('a named Device never sets up again', () => {
    expect(setupCandidate([sampleDevice('a', 'Desk', connected)], new Set())).toBeNull();
  });

  test('a skipped Device stays skipped, and the next one gets its turn', () => {
    expect(setupCandidate([unnamed('a'), unnamed('b')], new Set(['a']))?.id).toBe('b');
    expect(setupCandidate([unnamed('a')], new Set(['a']))).toBeNull();
  });

  test('a Device that is not connected waits until it is', () => {
    expect(setupCandidate([unnamed('a', { state: 'offline' })], new Set())).toBeNull();
    expect(setupCandidate([unnamed('a', { state: 'connecting', port: 'COM3' })], new Set())).toBeNull();
    expect(setupCandidate([unnamed('a', { state: 'lost', reason: 'unplugged', detail: '', at_ms: 0 })], new Set())).toBeNull();
  });
});

describe('remembering a skip', () => {
  test('skips round-trip through storage', () => {
    const storage = memoryStorage();
    writeSkipped(storage, new Set(['a', 'b']));
    expect(readSkipped(storage)).toEqual(new Set(['a', 'b']));
  });

  test.each([null, '', 'not json', '{"a":1}', '[1,"a",null]'])('stored %j reads as what it can', (seed) => {
    expect(readSkipped(memoryStorage(seed))).toEqual(seed === '[1,"a",null]' ? new Set(['a']) : new Set());
  });

  test('no storage, or storage that refuses, forgets quietly', () => {
    expect(readSkipped(null)).toEqual(new Set());
    const full: SkipStorage = {
      getItem: () => null,
      setItem: () => {
        throw new Error('QuotaExceededError');
      },
    };
    expect(() => writeSkipped(full, new Set(['a']))).not.toThrow();
  });
});
