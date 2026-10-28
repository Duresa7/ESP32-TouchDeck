import { createCompanion } from './api.ts';
import { isSessionError, toApiError } from './errors.ts';
import type { Session } from './session.ts';

export type Companion = ReturnType<typeof createCompanion>;

/**
 * Sends one Companion call with the session loaded. Every failure becomes an
 * ApiError. A 401 asks the session for a new token and retries once.
 */
export async function request<T>(session: Session, send: () => Promise<T>): Promise<T> {
  const token = await session.token();
  try {
    return await send();
  } catch (first) {
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

// Wraps every `companion.<feature>.<action>()` in `request`, keeping its type.
function guard<T extends object>(api: T, session: Session): T {
  const guarded: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(api)) {
    guarded[key] =
      typeof value === 'function'
        ? (...args: unknown[]) => request(session, () => (value as (...a: unknown[]) => Promise<unknown>)(...args))
        : guard(value as object, session);
  }
  return guarded as T;
}

/** The typed Companion client: `companion.devices.list()`, `companion.system.get()`. */
export function createClient(base: string, session: Session): Companion {
  return guard(
    createCompanion(base, () => session.current() ?? ''),
    session,
  );
}
