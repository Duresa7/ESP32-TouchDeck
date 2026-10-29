import { isApiError } from '../../companion/errors.ts';

export const UNREACHABLE = "Can't reach the Companion. Check that Open TouchDeck is running in the tray, then retry.";

export function problemText(error: unknown, fallback: string): string {
  return isApiError(error, 'client.unreachable') ? UNREACHABLE : fallback;
}
