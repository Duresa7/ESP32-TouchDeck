import type { SessionInfo, SessionKind } from './api.ts';

export interface FakeBrowser {
  label: string;
  created_ms: number;
  last_used_ms: number;
}

export interface FakeSessionsOptions {
  browsers?: readonly FakeBrowser[];
  /** Called with the tokens a revoke or Reset pairing just ended. */
  onRevoke(tokens: ReadonlySet<string>): void;
}

export type TokenStatus = 'live' | 'revoked' | 'unknown';

const json = (status: number, body: unknown) =>
  new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

const failure = (status: number, code: string, message: string) => json(status, { error: { code, message, details: null } });

export function createFakeSessions({ browsers = [], onRevoke }: FakeSessionsOptions) {
  const live = new Map<string, SessionInfo>();
  const revoked = new Set<string>();
  let count = 0;

  function mint({ kind, label, created_ms = Date.now(), last_used_ms = created_ms }: { kind: SessionKind; label: string; created_ms?: number; last_used_ms?: number }): string {
    count += 1;
    const n = String(count).padStart(4, '0');
    const token = `tds_fake${n}`;
    live.set(token, { id: `session_fake${n}`, kind, label, created_ms, last_used_ms, current: false });
    return token;
  }

  for (const browser of browsers) mint({ kind: 'browser', ...browser });

  function revoke(tokens: ReadonlySet<string>) {
    for (const token of tokens) {
      live.delete(token);
      revoked.add(token);
    }
    if (tokens.size) onRevoke(tokens);
  }

  function handle(request: Request, path: string, caller: string): Response | null {
    if (path === '/auth/sessions') {
      if (request.method === 'DELETE') {
        const kinds = new URL(request.url).searchParams.getAll('kind');
        if (kinds.length !== 1 || kinds[0] !== 'browser') return failure(400, 'request.invalid', 'Expected kind=browser.');
        if (live.get(caller)?.kind !== 'window') return failure(403, 'auth.forbidden', 'A window session is required.');
        revoke(new Set([...live].filter(([, info]) => info.kind === 'browser').map(([token]) => token)));
        return json(204, null);
      }
      if (request.method !== 'GET') return failure(405, 'route.method_not_allowed', 'Use GET.');
      const list = [...live].map(([token, info]) => ({ ...info, current: token === caller }));
      return json(200, list.toSorted((a, b) => a.created_ms - b.created_ms));
    }
    const one = /^\/auth\/sessions\/([^/]+)$/.exec(path);
    if (one) {
      if (request.method !== 'DELETE') return failure(405, 'route.method_not_allowed', 'Use DELETE.');
      const id = decodeURIComponent(one[1]);
      const target = id === 'self' ? caller : [...live].find(([, info]) => info.id === id)?.[0];
      revoke(new Set(target === undefined ? [] : [target]));
      return json(204, null);
    }
    if (path === '/auth/reset') {
      if (request.method !== 'POST') return failure(405, 'route.method_not_allowed', 'Use POST.');
      revoke(new Set(live.keys()));
      return json(204, null);
    }
    return null;
  }

  return {
    mint,
    handle,
    status(token: string): TokenStatus {
      return live.has(token) ? 'live' : revoked.has(token) ? 'revoked' : 'unknown';
    },
    revokeAll() {
      revoke(new Set(live.keys()));
    },
    list(): SessionInfo[] {
      return [...live.values()];
    },
  };
}
