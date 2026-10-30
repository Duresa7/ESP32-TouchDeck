import { describe, expect, test } from 'vitest';
import { createFakeSessions } from './fake-sessions.ts';

function setup() {
  const sessions = createFakeSessions({ browsers: [{ label: 'Edge on Windows', created_ms: 1, last_used_ms: 1 }], onRevoke: () => {} });
  const window = sessions.mint({ kind: 'window', label: 'Open TouchDeck' });
  const [browser] = sessions.list().filter((info) => info.kind === 'browser');
  return { sessions, window, browser };
}

const forgetAll = (query: string) => new Request(`http://127.0.0.1/api/v1/auth/sessions${query}`, { method: 'DELETE' });

describe('fake Forget all', () => {
  test('a window with exactly kind=browser signs out every browser', () => {
    const { sessions, window } = setup();
    expect(sessions.handle(forgetAll('?kind=browser'), '/auth/sessions', window)?.status).toBe(204);
    expect(sessions.list().map((info) => info.kind)).toEqual(['window']);
  });

  test.each(['', '?kind=window', '?kind=Browser', '?kind=browser&kind=window', '?kind=browser&kind=browser'])(
    '%s is refused as the Companion refuses it, and nobody is signed out',
    async (query) => {
      const { sessions, window, browser } = setup();
      const response = sessions.handle(forgetAll(query), '/auth/sessions', window);
      expect(response?.status).toBe(400);
      expect((await response?.json()).error.code).toBe('request.invalid');
      expect(sessions.list()).toContainEqual(browser);
    },
  );
});
