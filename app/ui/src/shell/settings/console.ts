import { createStore } from 'zustand/vanilla';
import type { Companion } from '../../companion/client.ts';
import { isApiError } from '../../companion/errors.ts';
import { problemText } from './problems.ts';

export type ConsoleEntry = { kind: 'sent' | 'line'; text: string };

export type ConsoleNotice = { kind: 'forbidden'; line: string } | { kind: 'failed'; text: string } | null;

export interface ConsoleState {
  entries: ConsoleEntry[];
  notice: ConsoleNotice;
  send(line: string): Promise<'sent' | 'kept'>;
  receive(line: string): void;
}

export const CONSOLE_KEEPS = 500;

function failedText(error: unknown, name: string): string {
  if (isApiError(error, 'device.not_connected')) return `Plug in ${name} to use the console.`;
  if (isApiError(error, 'device.timeout')) return `${name} didn't answer. Unplug it and plug it back in.`;
  if (isApiError(error, 'device.busy')) return `${name} is busy. Wait a moment, then retry.`;
  if (isApiError(error, 'device.link')) return `Lost ${name}. Check the USB cable. It reconnects on its own.`;
  return problemText(error, `Couldn't send to ${name}. Check that it's plugged in, then retry.`);
}

export function createConsoleStore({ companion, device, name }: { companion: Pick<Companion, 'console'>; device: string; name: string }) {
  return createStore<ConsoleState>()((set) => {
    const append = (entry: ConsoleEntry) => set((state) => ({ entries: [...state.entries, entry].slice(-CONSOLE_KEEPS) }));

    return {
      entries: [],
      notice: null,
      receive: (text) => append({ kind: 'line', text }),

      async send(line) {
        const text = line.trim();
        if (!text) return 'kept';
        const sent: ConsoleEntry = { kind: 'sent', text };
        set({ notice: null });
        append(sent);
        try {
          await companion.console.send({ device }, null, { line: text });
          return 'sent';
        } catch (error) {
          set((state) => ({
            entries: state.entries.filter((entry) => entry !== sent),
            notice: isApiError(error, 'console.forbidden') ? { kind: 'forbidden', line: text } : { kind: 'failed', text: failedText(error, name) },
          }));
          return 'kept';
        }
      },
    };
  });
}

export type ConsoleStore = ReturnType<typeof createConsoleStore>;
