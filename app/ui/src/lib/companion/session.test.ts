import { expect, test, vi } from 'vitest';
import { windowSession, type OpenTouchDeckBridge } from './session';

test('renewal names the stale token and notifications only read the session', async () => {
  let changed = () => {};
  const session = vi.fn<(stale?: string) => Promise<string | null>>()
    .mockResolvedValueOnce('first')
    .mockResolvedValue('second');
  const bridge: OpenTouchDeckBridge = {
    version: 1, session,
    onSessionChanged: callback => { changed = callback; },
  };
  const client = windowSession(bridge);
  expect(await client.token()).toBe('first');
  expect(await Promise.all([client.renew('first'), client.renew('first')])).toEqual(['second', 'second']);
  expect(session.mock.calls).toEqual([[undefined], ['first']]);
  expect(await client.renew('first')).toBe('second');
  changed();
  await Promise.resolve();
  expect(session.mock.calls).toEqual([[undefined], ['first'], [undefined]]);
  expect(client.current()).toBe('second');
});
