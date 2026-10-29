import { createStore } from 'zustand/vanilla';
import type { SessionInfo } from '../../companion/api.ts';
import type { Companion } from '../../companion/client.ts';
import { problemText } from './problems.ts';

export type BrowsersView = { kind: 'loading' } | { kind: 'ready'; browsers: SessionInfo[] } | { kind: 'failed' };

export interface BrowsersState {
  view: BrowsersView;
  problem: string | null;
  load(): Promise<void>;
  forget(browser: SessionInfo): Promise<void>;
  forgetAll(): Promise<void>;
  resetPairing(): Promise<void>;
}

const LOAD_PROBLEM = "Couldn't list paired browsers. Restart Open TouchDeck, then retry.";
const RESET_PROBLEM = "Couldn't reset pairing. Retry, or restart Open TouchDeck.";
const FORGET_ALL_PROBLEM = "Couldn't forget every browser. Retry, or restart Open TouchDeck.";
const forgetProblem = (browser: SessionInfo) => `Couldn't forget ${browser.label}. Retry, or restart Open TouchDeck.`;

const byAge = (a: SessionInfo, b: SessionInfo) => a.created_ms - b.created_ms;

export function createBrowsersStore(companion: Pick<Companion, 'auth'>) {
  return createStore<BrowsersState>()((set, get) => {
    const browsers = () => {
      const { view } = get();
      return view.kind === 'ready' ? view.browsers : [];
    };

    async function load() {
      try {
        const sessions = await companion.auth.sessions();
        set({ view: { kind: 'ready', browsers: sessions.filter((s) => s.kind === 'browser').toSorted(byAge) } });
      } catch (error) {
        set({ view: { kind: 'failed' }, problem: problemText(error, LOAD_PROBLEM) });
      }
    }

    return {
      view: { kind: 'loading' },
      problem: null,
      load,

      async forget(browser) {
        set({ view: { kind: 'ready', browsers: browsers().filter((b) => b.id !== browser.id) }, problem: null });
        try {
          await companion.auth.session_delete({ id: browser.id });
        } catch (error) {
          const back = [...browsers().filter((b) => b.id !== browser.id), browser].toSorted(byAge);
          set({ view: { kind: 'ready', browsers: back }, problem: problemText(error, forgetProblem(browser)) });
        }
      },

      async forgetAll() {
        set({ problem: null });
        try {
          await companion.auth.sessions_delete({}, { kind: 'browser' });
        } catch (error) {
          await load();
          set({ problem: problemText(error, FORGET_ALL_PROBLEM) });
          return;
        }
        await load();
      },

      async resetPairing() {
        set({ problem: null });
        try {
          await companion.auth.reset();
        } catch (error) {
          set({ problem: problemText(error, RESET_PROBLEM) });
          return;
        }
        await load();
      },
    };
  });
}

export type BrowsersStore = ReturnType<typeof createBrowsersStore>;
