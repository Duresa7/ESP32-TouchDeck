import type { Environment } from './connection.ts';
import { openWebSocket } from './events.ts';
import { windowSession, type OpenTouchDeckBridge } from './session.ts';

/** The dev server's stand-in for Electron main's session handout (dev/companion.ts). */
export const DEV_SESSION_PATH = '/__companion/session';

// `pnpm dev:ui` runs the UI on Vite against the Companion on this PC. Vite proxies /api
// to it, and mints a window session for the page the way Electron main would.
export function devEnvironment(target: Window): Environment {
  const bridge: OpenTouchDeckBridge = {
    version: 1,
    async session() {
      const response = await target.fetch(DEV_SESSION_PATH, { method: 'POST', headers: { 'X-TouchDeck-Dev': '1' } });
      const body = (await response.json()) as { token?: string; error?: string };
      // A Companion that isn't up yet is a failure to retry, not a browser that has to pair.
      if (!response.ok || !body.token) throw new TypeError(body.error ?? "Couldn't get a session from the dev server.");
      return body.token;
    },
    onSessionChanged: () => {},
  };
  return { base: target.location.origin, session: windowSession(bridge), open: openWebSocket };
}
