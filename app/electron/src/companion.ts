import { createHmac, randomBytes, timingSafeEqual } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

export const restartMessage = 'Restart Open TouchDeck to finish updating';
type Runtime = { version: number; api: string[]; port: number; token: string; started_at_ms: number };
export type WindowSession = { origin: string; token: string };

export function runtimePath(env = process.env, platform = process.platform): string {
  if (env.TOUCHDECK_APP_DATA_DIR) return join(env.TOUCHDECK_APP_DATA_DIR, 'runtime', 'companion.json');
  if (platform === 'linux' && env.XDG_RUNTIME_DIR) return join(env.XDG_RUNTIME_DIR, 'touchdeck', 'companion.json');
  if (platform === 'win32' && env.LOCALAPPDATA) return join(env.LOCALAPPDATA, 'io.github.duresa7.touchdeck', 'runtime', 'companion.json');
  throw new Error('Companion runtime directory is unavailable');
}

async function readRuntime(path: string): Promise<Runtime> {
  const value = JSON.parse(await readFile(path, 'utf8'));
  if (value.version !== 1 || !Array.isArray(value.api)) throw new Error(restartMessage);
  if (!Number.isInteger(value.port) || value.port < 1 || value.port > 65535 ||
      !/^tdp_[A-Za-z0-9_-]{43}$/.test(value.token) || !Number.isSafeInteger(value.started_at_ms)) {
    throw new Error('Invalid Companion runtime file');
  }
  return value;
}

async function request(origin: string, path: string, init: RequestInit = {}) {
  const response = await fetch(`${origin}/api/v1${path}`, {
    ...init, redirect: 'error', signal: AbortSignal.timeout(5000),
    headers: { 'Content-Type': 'application/json', ...init.headers },
  });
  if (!response.ok) throw new Error('Companion request failed');
  return response;
}

async function prove(runtime: Runtime): Promise<string> {
  const origin = `http://127.0.0.1:${runtime.port}`;
  const nonce = randomBytes(32);
  const { proof } = await (await request(origin, '/auth/proof', {
    method: 'POST', body: JSON.stringify({ nonce: nonce.toString('base64url') }),
  })).json();
  const expected = createHmac('sha256', runtime.token).update('touchdeck-companion-proof\0').update(nonce).digest();
  if (typeof proof !== 'string' || !/^[A-Za-z0-9_-]{43}$/.test(proof)) throw new Error('Invalid Companion proof');
  const actual = Buffer.from(proof, 'base64url');
  if (actual.length !== expected.length || !timingSafeEqual(actual, expected)) throw new Error('Invalid Companion proof');
  return origin;
}

export async function connect(path: string, start: () => void, timeout = 10_000): Promise<WindowSession> {
  let runtime: Runtime | undefined;
  let origin: string | undefined;
  try {
    runtime = await readRuntime(path);
    origin = await prove(runtime);
  } catch {
    const previous = runtime?.started_at_ms ?? -1;
    start();
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      try {
        runtime = await readRuntime(path);
        if (runtime.started_at_ms > previous) {
          origin = await prove(runtime);
          break;
        }
      } catch { /* The Companion publishes the runtime file atomically after binding. */ }
      await delay(100);
    }
  }
  if (!runtime || !origin) throw new Error('Could not verify the Companion');
  const ping = await (await request(origin, '/system/ping')).json();
  if (!runtime.api.includes('v1') || !Array.isArray(ping.api) || !ping.api.includes('v1')) throw new Error(restartMessage);
  const session = await (await request(origin, '/auth/sessions', {
    method: 'POST', headers: { Authorization: `Bearer ${runtime.token}` }, body: JSON.stringify({ kind: 'window' }),
  })).json();
  if (!/^tds_[A-Za-z0-9_-]{43}$/.test(session.token)) throw new Error('Invalid window session');
  return { origin, token: session.token };
}

export async function revoke(session: WindowSession): Promise<void> {
  await request(session.origin, '/auth/sessions/self', { method: 'DELETE', headers: { Authorization: `Bearer ${session.token}` } });
}
