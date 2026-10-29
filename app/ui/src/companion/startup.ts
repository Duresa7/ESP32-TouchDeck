// Hand-written mirror of system.startup_get and system.startup_put from #79, shaped like
// the generated client. Delete this file once #79's generated api.gen.ts carries them.

export type StartupSetting = { at_sign_in: boolean };

export function createStartupRoutes(base: string, token: () => string) {
  async function call(method: 'GET' | 'PUT', body: StartupSetting | null, bearer?: string, headers?: Record<string, string>): Promise<StartupSetting> {
    const auth = bearer ?? token();
    const response = await fetch(new URL('/api/v1/system/startup', base), {
      method,
      headers: { ...headers, ...(auth ? { Authorization: `Bearer ${auth}` } : {}), ...(body == null ? {} : { 'Content-Type': 'application/json' }) },
      ...(body == null ? {} : { body: JSON.stringify(body) }),
    });
    if (!response.ok) throw (await response.json()).error;
    return response.json();
  }
  return {
    startup_get: (_path: Record<string, never> = {}, _query: null = null, _body: null = null, bearer?: string, headers?: Record<string, string>) => call('GET', null, bearer, headers),
    startup_put: (_path: Record<string, never> = {}, _query: null = null, body: StartupSetting, bearer?: string, headers?: Record<string, string>) => call('PUT', body, bearer, headers),
  };
}
