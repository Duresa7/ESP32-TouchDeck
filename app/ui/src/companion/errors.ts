import type { ErrorCode } from './api.ts';

// Codes the client makes up when no Companion error body exists: the request
// never got an answer, or the answer wasn't the one error shape.
export type ClientErrorCode = 'client.unreachable' | 'client.bad_response';

// An unknown code from a newer Companion is still a string; the UI treats it as generic.
export type AnyErrorCode = ErrorCode | ClientErrorCode | (string & {});

// The codes a 401 carries. Any of them means "get a new session, then retry once".
const SESSION_CODES: ReadonlySet<string> = new Set(['auth.missing', 'auth.invalid', 'auth.revoked', 'auth.expired']);

export class ApiError extends Error {
  readonly code: AnyErrorCode;
  readonly details: unknown;

  constructor(code: AnyErrorCode, message: string, details: unknown = null) {
    super(message);
    this.name = 'ApiError';
    this.code = code;
    this.details = details;
  }
}

function isErrorBody(value: unknown): value is { code: string; message: string; details?: unknown } {
  if (typeof value !== 'object' || value === null) return false;
  const body = value as Record<string, unknown>;
  return typeof body.code === 'string' && typeof body.message === 'string';
}

// Turns whatever a Companion call threw into an ApiError: the error body itself,
// a whole `{error: {...}}` response, a body that wasn't JSON, or a failed fetch.
export function toApiError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  if (isErrorBody(error)) return new ApiError(error.code, error.message, error.details ?? null);
  if (typeof error === 'object' && error !== null && isErrorBody((error as { error?: unknown }).error)) {
    return toApiError((error as { error: unknown }).error);
  }
  // fetch rejects with a TypeError when no answer arrives at all.
  if (error instanceof TypeError) return new ApiError('client.unreachable', "Couldn't reach the Companion.");
  return new ApiError('client.bad_response', "The Companion's answer wasn't the error shape.");
}

export function isApiError<C extends AnyErrorCode>(error: unknown, code?: C): error is ApiError & { code: C } {
  return error instanceof ApiError && (code === undefined || error.code === code);
}

export function isSessionError(error: unknown): boolean {
  return error instanceof ApiError && SESSION_CODES.has(error.code);
}
