import type { Environment } from './connection.ts';
import { ApiError } from './errors.ts';
import { openWebSocket } from './events.ts';
import { windowSession, type OpenTouchDeckBridge } from './session.ts';

export const DEV_SESSION_PATH = '/__companion/session';

export function devEnvironment(target: Window): Environment {
  const bridge: OpenTouchDeckBridge = {
    version: 1,
    async session() {
      const response = await target.fetch(DEV_SESSION_PATH, { method: 'POST', headers: { 'X-TouchDeck-Dev': '1' } });
      const body: unknown = await response.json();
      const answer = typeof body === 'object' && body !== null ? body : {};
      const token = 'token' in answer && typeof answer.token === 'string' ? answer.token : '';
      if (response.ok && token) return token;
      throw new ApiError('client.unreachable', 'error' in answer && typeof answer.error === 'string' ? answer.error : "Couldn't get a session from the dev server.");
    },
    onSessionChanged: () => {},
  };
  return { base: target.location.origin, session: windowSession(bridge), open: openWebSocket };
}
