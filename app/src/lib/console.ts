export type ConsoleEntry =
  | { kind: 'sent'; text: string }
  | { kind: 'reply'; text: string; ok: boolean }
  | { kind: 'log'; text: string };

export const CONSOLE_LIMIT = 500;

const PROMPT = /^(?:td> )+/;
const REPLY = /@td (\{.*\})\s*$/;

/** Reads one console line: an `@td` reply, or a log line with any leading prompt removed. */
export function classify(line: string): ConsoleEntry | null {
  const text = line.replace(PROMPT, '').trimEnd();
  if (!text) return null;
  const match = REPLY.exec(text);
  if (match) {
    try {
      const reply: unknown = JSON.parse(match[1]);
      if (reply !== null && typeof reply === 'object' && !Array.isArray(reply)) {
        return { kind: 'reply', text: match[1], ok: (reply as { ok?: unknown }).ok === true };
      }
    } catch {
      return { kind: 'log', text };
    }
  }
  return { kind: 'log', text };
}

/** Appends an entry and drops the oldest ones past the limit. */
export function append(entries: ConsoleEntry[], entry: ConsoleEntry | null, limit = CONSOLE_LIMIT): ConsoleEntry[] {
  if (!entry) return entries;
  return [...entries.slice(Math.max(0, entries.length - limit + 1)), entry];
}
