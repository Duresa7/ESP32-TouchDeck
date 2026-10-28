// `pnpm dev:ui` serves the UI from Vite and talks to the Companion running on this PC
// (docs/companion-api.md, "Embedded UI" and "How the Electron window gets its session").
//
// - /api, the events WebSocket included, is proxied to http://127.0.0.1:PORT with Host
//   and Origin rewritten to the Companion's origin, which is the only one it accepts.
//   Only requests from the dev page itself are proxied, so other sites can't borrow
//   the rewrite.
// - POST /__companion/session plays Electron main's part: it reads companion.json,
//   checks the Companion's HMAC proof, mints a window session with the pairing token,
//   and hands the page only the session token.
import { createHmac, randomBytes, timingSafeEqual } from 'node:crypto';
import { readFileSync } from 'node:fs';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { join } from 'node:path';
import type { Plugin, ProxyOptions } from 'vite';
import { createCompanion } from '../src/companion/api.gen.ts';
import { DEV_SESSION_PATH } from '../src/companion/dev.ts';

const APP_ID = 'io.github.duresa7.touchdeck';

type Env = Record<string, string | undefined>;

// The Companion's data folder and runtime folder, as td-companion's process.rs finds them.
function dataDir(env: Env): string | null {
  if (env.TOUCHDECK_APP_DATA_DIR) return env.TOUCHDECK_APP_DATA_DIR;
  if (process.platform === 'win32') return env.LOCALAPPDATA ? join(env.LOCALAPPDATA, APP_ID) : null;
  if (process.platform === 'darwin') return env.HOME ? join(env.HOME, 'Library', 'Application Support', APP_ID) : null;
  const share = env.XDG_DATA_HOME ?? (env.HOME ? join(env.HOME, '.local', 'share') : null);
  return share ? join(share, APP_ID) : null;
}

function runtimeDir(env: Env): string | null {
  if (env.TOUCHDECK_APP_DATA_DIR) return join(env.TOUCHDECK_APP_DATA_DIR, 'runtime');
  if (process.platform === 'win32') return env.LOCALAPPDATA ? join(env.LOCALAPPDATA, APP_ID, 'runtime') : null;
  return env.XDG_RUNTIME_DIR ? join(env.XDG_RUNTIME_DIR, 'touchdeck') : null;
}

function readJson(path: string | null): Record<string, unknown> | null {
  if (!path) return null;
  try {
    return JSON.parse(readFileSync(path, 'utf8')) as Record<string, unknown>;
  } catch {
    return null;
  }
}

/** The running Companion's port and pairing token, from companion.json. */
function readRuntime(env: Env): { port: number; token: string } | null {
  const dir = runtimeDir(env);
  const file = readJson(dir && join(dir, 'companion.json'));
  return typeof file?.port === 'number' && typeof file.token === 'string' ? { port: file.port, token: file.token } : null;
}

/** The port to proxy to: the running Companion's, or the one it saved for its next start. */
function companionPort(env: Env): number | null {
  const running = readRuntime(env)?.port;
  if (running) return running;
  const dir = dataDir(env);
  const saved = readJson(dir && join(dir, 'companion-settings.json'))?.port;
  return typeof saved === 'number' ? saved : null;
}

/** A request from the dev page: same origin as the dev server, or no browser context at all. */
function fromDevPage(req: IncomingMessage): boolean {
  const { host, origin } = req.headers;
  const site = req.headers['sec-fetch-site'];
  if (!host?.startsWith('127.0.0.1:')) return false;
  if (origin !== undefined && origin !== `http://${host}`) return false;
  return site === undefined || site === 'same-origin' || site === 'none';
}

/** Mints a window session the way Electron main does, proof first. */
async function mintSession(env: Env): Promise<string> {
  const runtime = readRuntime(env);
  if (!runtime) throw new Error('No companion.json. Start the Companion first.');
  const companion = createCompanion(`http://127.0.0.1:${runtime.port}`, () => '');
  const nonce = randomBytes(32);
  const { proof } = await companion.auth.proof({}, null, { nonce: nonce.toString('base64url') });
  const expected = createHmac('sha256', runtime.token).update('touchdeck-companion-proof\0').update(nonce).digest();
  const answer = Buffer.from(proof, 'base64url');
  if (answer.length !== expected.length || !timingSafeEqual(answer, expected)) {
    throw new Error("The process on the Companion's port couldn't prove it's the Companion.");
  }
  const created = await companion.auth.session_create({}, null, { kind: 'window', label: 'Vite dev server' }, runtime.token);
  return created.token;
}

function send(res: ServerResponse, status: number, body: unknown) {
  res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
  res.end(JSON.stringify(body));
}

export function companionDev(env: Env = process.env): Plugin {
  return {
    name: 'companion-dev',
    apply: (_, { command, mode }) => command === 'serve' && mode !== 'demo',

    config() {
      const port = companionPort(env);
      if (port === null) {
        console.warn('[companion-dev] No Companion port found. Start the Companion, then restart the dev server.');
        return;
      }
      const target = `http://127.0.0.1:${port}`;
      const proxy: ProxyOptions = {
        target,
        ws: true,
        // Host becomes 127.0.0.1:PORT, and so does Origin, on requests and the WebSocket upgrade.
        changeOrigin: true,
        rewriteWsOrigin: true,
        configure(server) {
          server.on('proxyReq', (proxyReq, req) => {
            if (req.headers.origin !== undefined) proxyReq.setHeader('origin', target);
          });
        },
        // Vite runs this for the WebSocket upgrade too. Anything else gets a 404.
        bypass: (req) => (fromDevPage(req) ? undefined : false),
      };
      return { server: { proxy: { '/api': proxy } } };
    },

    configureServer(server) {
      // Runs before Vite's own middleware.
      server.middlewares.use((req, res, next) => {
        if (req.url?.split('?')[0] !== DEV_SESSION_PATH) return next();
        if (req.method !== 'POST' || req.headers['x-touchdeck-dev'] !== '1' || !fromDevPage(req)) return send(res, 403, { error: 'forbidden' });
        mintSession(env).then(
          (token) => send(res, 200, { token }),
          // The generated client throws the Companion's error body, which isn't an Error.
          (error: unknown) => send(res, 503, { error: (error as { message?: unknown })?.message ?? String(error) }),
        );
      });
    },
  };
}
