import { createCompanion } from './api.ts';
import { ApiError, isSessionError, toApiError } from './errors.ts';
import type { Session } from './session.ts';

export type Companion = ReturnType<typeof createCompanion>;

export const isAbort = (error: unknown): error is DOMException => error instanceof DOMException && error.name === 'AbortError';

export async function request<T>(session: Session, send: () => Promise<T>): Promise<T> {
  let token: string | null;
  try {
    token = await session.token();
  } catch (error) {
    throw new ApiError('client.unreachable', error instanceof Error ? error.message : "Couldn't get a session.");
  }
  try {
    return await send();
  } catch (first) {
    if (isAbort(first)) throw first;
    const error = toApiError(first);
    if (!isSessionError(error)) throw error;
    const fresh = await session.renew(token);
    if (fresh === null) throw error;
    try {
      return await send();
    } catch (second) {
      throw toApiError(second);
    }
  }
}

function guard<T extends object>(api: T, session: Session): T {
  const guarded: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(api)) {
    const member: unknown = value;
    if (typeof member === 'function') guarded[key] = (...args: unknown[]) => request(session, async (): Promise<unknown> => member(...args));
    else if (typeof member === 'object' && member !== null) guarded[key] = guard(member, session);
    else guarded[key] = member;
  }
  return guarded as T;
}

export function createClient(base: string, session: Session): Companion {
  return guard(
    createCompanion(base, () => session.current() ?? ''),
    session,
  );
}
