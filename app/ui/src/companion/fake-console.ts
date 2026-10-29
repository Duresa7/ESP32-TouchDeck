import type { DeviceSummary } from './api.ts';
import { consoleCommands } from './console.ts';

type Frame = Record<string, unknown> & { type: string };

export interface FakeConsoleOptions {
  devices: Map<string, DeviceSummary>;
  publish(frame: Frame): void;
}

const json = (status: number, body: unknown) =>
  new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

const failure = (status: number, code: string, message: string, details: unknown = null) => json(status, { error: { code, message, details } });

const SPECIAL = /["'\\`;&|$<>#]/;

export function isAllowedConsoleLine(line: string): boolean {
  if (line.length === 0 || line.length > 256 || !/^[\x20-\x7e]+$/.test(line) || SPECIAL.test(line)) return false;
  const words = line.split(' ');
  if (words.includes('')) return false;
  return consoleCommands.some((form) => {
    const parts = form.split(' ');
    return parts.length === words.length && parts.every((part, i) => part.startsWith('<') || part === words[i]);
  });
}

function reply(line: string, device: DeviceSummary): Record<string, unknown> {
  const [cmd] = line.split(' ');
  switch (line) {
    case 'version':
      return { cmd, ok: true, project_name: 'touchdeck', version: device.firmware?.version ?? '', elf_sha256: device.firmware?.build ?? '' };
    case 'heap':
      return { cmd, ok: true, free_heap: 182_344 };
    case 'uptime':
      return { cmd, ok: true, uptime_ms: 3_725_000 };
    default:
      return { cmd, ok: true };
  }
}

export function createFakeConsole({ devices, publish }: FakeConsoleOptions) {
  const sent: { device: string; line: string }[] = [];

  async function handle(request: Request, path: string): Promise<Response | null> {
    const route = /^\/devices\/([^/]+)\/console$/.exec(path);
    if (!route) return null;
    if (request.method !== 'POST') return failure(405, 'route.method_not_allowed', 'Use POST.');
    const id = decodeURIComponent(route[1]);
    const device = devices.get(id);
    if (!device) return failure(404, 'device.not_found', 'No Device with that id.');
    const body: unknown = await request.json().catch(() => null);
    const line = typeof body === 'object' && body !== null && 'line' in body && typeof body.line === 'string' ? body.line : null;
    if (line === null) return failure(400, 'request.invalid', 'Expected {line}.', { part: 'body', reason: 'missing line' });
    if (!isAllowedConsoleLine(line)) return failure(403, 'console.forbidden', "The console line isn't on the read-only allowlist.");
    if (device.link.state !== 'connected') return failure(409, 'device.not_connected', 'The Device is not connected.', { link: device.link });
    sent.push({ device: id, line });
    setTimeout(() => publish({ type: 'device.console', device: id, data: { device: id, line: `@td ${JSON.stringify(reply(line, device))}` } }), 0);
    return json(204, null);
  }

  return { handle, sent };
}
