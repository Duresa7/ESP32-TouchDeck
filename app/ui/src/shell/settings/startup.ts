import { createStore } from 'zustand/vanilla';
import type { Companion } from '../../companion/client.ts';
import { problemText } from './problems.ts';

export type StartupView =
  | { kind: 'loading' }
  | { kind: 'ready'; on: boolean; problem: string | null }
  | { kind: 'unreadable'; problem: string };

export interface StartupState {
  view: StartupView;
  load(): Promise<void>;
  set(on: boolean): Promise<void>;
}

const writeProblem = (on: boolean) => `Couldn't turn ${on ? 'on' : 'off'} Start at sign-in. Retry, or restart Open TouchDeck.`;
const READ_PROBLEM = "Couldn't read Start at sign-in. Restart Open TouchDeck, then retry.";

export function createStartupStore(companion: Pick<Companion, 'system'>) {
  let saved = false;
  let latest = 0;
  let writes = Promise.resolve();

  return createStore<StartupState>()((set, get) => ({
    view: { kind: 'loading' },

    async load() {
      try {
        saved = (await companion.system.startup_get()).at_sign_in;
        set({ view: { kind: 'ready', on: saved, problem: null } });
      } catch (error) {
        set({ view: { kind: 'unreadable', problem: problemText(error, READ_PROBLEM) } });
      }
    },

    set(on) {
      if (get().view.kind !== 'ready') return writes;
      const write = ++latest;
      set({ view: { kind: 'ready', on, problem: null } });
      writes = writes.then(async () => {
        try {
          saved = (await companion.system.startup_put({}, null, { at_sign_in: on })).at_sign_in;
          if (write === latest) set({ view: { kind: 'ready', on: saved, problem: null } });
        } catch (error) {
          if (write === latest) set({ view: { kind: 'ready', on: saved, problem: problemText(error, writeProblem(on)) } });
        }
      });
      return writes;
    },
  }));
}

export type StartupStore = ReturnType<typeof createStartupStore>;
