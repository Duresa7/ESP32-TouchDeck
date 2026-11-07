// The Brandfetch settings the Companion keeps: the client ID (not a secret) and whether an API
// key is stored (the key itself never comes back). One store per Companion, shared by the
// Settings section and the icon picker.
import { createStore, type StoreApi } from 'zustand/vanilla';
import type { BrandfetchStatus } from '../companion/api.ts';
import type { Companion } from '../companion/client.ts';
import { isApiError } from '../companion/errors.ts';

export const DEVELOPER_PORTAL = 'https://developers.brandfetch.com/register';

export type BrandfetchView =
  | { kind: 'loading' }
  | { kind: 'ready'; status: BrandfetchStatus; problem: string | null; notice: string | null; busy: boolean }
  | { kind: 'unreadable'; problem: string };

export interface BrandfetchState {
  view: BrandfetchView;
  load(): Promise<void>;
  saveClientId(id: string): Promise<boolean>;
  forgetClientId(): Promise<boolean>;
  saveKey(value: string): Promise<boolean>;
  forgetKey(): Promise<boolean>;
  /** One Brand API call, on request only: saving never calls Brandfetch. */
  testKey(): Promise<boolean>;
}

const UNREACHABLE = "Can't reach the Companion. Check that Open TouchDeck is running in the tray, then retry.";
const READ_PROBLEM = "Couldn't read the Brandfetch settings. Restart Open TouchDeck, then retry.";
const BAD_CLIENT_ID = "That doesn't look like a client ID. Copy it from the Brandfetch developer portal.";
const BAD_KEY = "That doesn't look like an API key. Copy it from the Brandfetch developer portal.";
const SAVE_PROBLEM = "Couldn't save the Brandfetch settings. Retry, or restart Open TouchDeck.";
const KEY_WORKS = 'Brandfetch accepted the key.';

function problemOf(error: unknown, invalid: string): string {
  if (isApiError(error, 'client.unreachable')) return UNREACHABLE;
  if (isApiError(error, 'request.invalid')) return invalid;
  // The Companion names this platform's credential store and how to start it.
  if (isApiError(error, 'brandfetch.no_keychain')) return error.message;
  return SAVE_PROBLEM;
}

function testProblem(error: unknown): string {
  if (isApiError(error, 'client.unreachable')) return UNREACHABLE;
  if (isApiError(error, 'brandfetch.no_keychain')) return error.message;
  if (isApiError(error, 'brandfetch.no_key')) return 'Save an API key first.';
  if (isApiError(error, 'brandfetch.key_refused')) return 'Brandfetch refused the key. Replace it with the API key from the Brandfetch developer portal.';
  if (isApiError(error, 'brandfetch.quota')) return 'Your Brandfetch API requests are used up. Check your plan in the Brandfetch developer portal.';
  return "Brandfetch didn't answer. Check the internet connection, then retry.";
}

export function createBrandfetchStore(companion: Pick<Companion, 'brandfetch'>) {
  return createStore<BrandfetchState>()((set, get) => {
    const write = async (send: () => Promise<BrandfetchStatus>, invalid: string): Promise<boolean> => {
      const view = get().view;
      if (view.kind !== 'ready' || view.busy) return false;
      set({ view: { ...view, busy: true, problem: null, notice: null } });
      try {
        set({ view: { kind: 'ready', status: await send(), problem: null, notice: null, busy: false } });
        return true;
      } catch (error) {
        set({ view: { ...view, busy: false, problem: problemOf(error, invalid), notice: null } });
        return false;
      }
    };
    return {
      view: { kind: 'loading' },
      async load() {
        try {
          const status = await companion.brandfetch.get();
          const now = get().view;
          const kept = now.kind === 'ready' ? now : { problem: null, notice: null, busy: false };
          set({ view: { kind: 'ready', status, problem: kept.problem, notice: kept.notice, busy: kept.busy } });
        } catch (error) {
          if (get().view.kind !== 'ready') set({ view: { kind: 'unreadable', problem: isApiError(error, 'client.unreachable') ? UNREACHABLE : READ_PROBLEM } });
        }
      },
      saveClientId: (id) => write(() => companion.brandfetch.client_id_put({}, null, { client_id: id.trim() }), BAD_CLIENT_ID),
      forgetClientId: () => write(() => companion.brandfetch.client_id_delete(), BAD_CLIENT_ID),
      saveKey: (value) => write(() => companion.brandfetch.key_put({}, null, { value }), BAD_KEY),
      forgetKey: () => write(() => companion.brandfetch.key_delete(), BAD_KEY),
      async testKey() {
        const view = get().view;
        if (view.kind !== 'ready' || view.busy) return false;
        set({ view: { ...view, busy: true, problem: null, notice: null } });
        try {
          set({ view: { kind: 'ready', status: await companion.brandfetch.key_test(), problem: null, notice: KEY_WORKS, busy: false } });
          return true;
        } catch (error) {
          set({ view: { ...view, busy: false, problem: testProblem(error), notice: null } });
          return false;
        }
      },
    };
  });
}

export type BrandfetchStore = StoreApi<BrandfetchState>;

const stores = new WeakMap<object, BrandfetchStore>();

/** The shared store for this Companion client. */
export function brandfetchStore(companion: Pick<Companion, 'brandfetch'>): BrandfetchStore {
  let store = stores.get(companion);
  if (!store) {
    store = createBrandfetchStore(companion);
    stores.set(companion, store);
  }
  return store;
}

/** The client ID search runs with, or null while Brandfetch is off or unread. */
export function clientIdOf(view: BrandfetchView): string | null {
  return view.kind === 'ready' ? view.status.client_id : null;
}

/** "3 logos; 1 needs a refresh", or null when there are none. */
export function logoCount(status: Pick<BrandfetchStatus, 'logos' | 'need_refresh'>): string | null {
  if (status.logos === 0) return null;
  const logos = `${status.logos} ${status.logos === 1 ? 'logo' : 'logos'}`;
  if (status.need_refresh === 0) return `${logos}, all up to date`;
  return `${logos}; ${status.need_refresh} ${status.need_refresh === 1 ? 'needs' : 'need'} a refresh`;
}
