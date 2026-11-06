import type { StartupSetting } from './api.gen.ts';

const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

const failure = (status: number, code: string, message: string) => json(status, { error: { code, message, details: null } });

export function createFakeStartup(initial = false) {
  let setting: StartupSetting = { at_sign_in: initial };
  let failing = false;

  async function handle(request: Request, path: string): Promise<Response | null> {
    if (path !== '/system/startup') return null;
    if (request.method === 'GET') return json(200, setting);
    if (request.method !== 'PUT') return failure(405, 'route.method_not_allowed', 'Use GET or PUT.');
    const body: unknown = await request.json().catch(() => null);
    if (typeof body !== 'object' || body === null || !('at_sign_in' in body) || typeof body.at_sign_in !== 'boolean') {
      return failure(400, 'request.invalid', 'Expected {at_sign_in}.');
    }
    if (failing) return failure(500, 'internal', 'Internal error.');
    setting = { at_sign_in: body.at_sign_in };
    return json(200, setting);
  }

  return {
    handle,
    get atSignIn() {
      return setting.at_sign_in;
    },
    /** While on, every write fails with 500 `internal`, as a refused registry write would. */
    fail(on: boolean) {
      failing = on;
    },
  };
}
