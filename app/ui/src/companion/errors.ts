import type { ErrorCode } from './api.ts';

export type ClientErrorCode = 'client.unreachable' | 'client.bad_response';

export type AnyErrorCode = ErrorCode | ClientErrorCode | (string & {});

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
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    typeof value.code === 'string' &&
    'message' in value &&
    typeof value.message === 'string'
  );
}

export function toApiError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  if (isErrorBody(error)) return new ApiError(error.code, error.message, error.details ?? null);
  if (typeof error === 'object' && error !== null && 'error' in error && isErrorBody(error.error)) {
    return toApiError(error.error);
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
