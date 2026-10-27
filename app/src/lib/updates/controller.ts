export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes: string;
}

export type Progress = { downloaded: number; total: number | null };
type UpdateErrorKind = 'unconfigured' | 'unsupported' | 'network' | 'signature' | 'metadata' | 'busy' | 'installer' | 'internal';
export interface UpdateError { kind: UpdateErrorKind; message: string }

export function parseUpdateError(error: unknown): UpdateError {
  if (typeof error === 'object' && error !== null && 'kind' in error && 'message' in error && typeof error.message === 'string') {
    switch (error.kind) {
      case 'unconfigured': case 'unsupported': case 'network': case 'signature':
      case 'metadata': case 'busy': case 'installer': case 'internal':
        return { kind: error.kind, message: error.message };
    }
  }
  return { kind: 'internal', message: String(error) };
}

export type UpdateState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'current' }
  | { kind: 'available'; update: UpdateInfo }
  | { kind: 'downloading'; update: UpdateInfo; progress: Progress }
  | { kind: 'ready'; update: UpdateInfo }
  | { kind: 'installing'; update: UpdateInfo }
  | { kind: 'error'; error: UpdateError };

interface UpdateApi {
  check(): Promise<UpdateInfo | null>;
  download(progress: (value: Progress) => void): Promise<void>;
  install(): Promise<void>;
}

export function createAppUpdater({ api, changed }: { api: UpdateApi; changed: (state: UpdateState) => void }) {
  let state: UpdateState = { kind: 'idle' };
  function set(next: UpdateState) {
    state = next;
    changed(next);
  }
  function fail(error: unknown) {
    set({ kind: 'error', error: parseUpdateError(error) });
  }
  return {
    async check() {
      if (state.kind === 'checking' || state.kind === 'downloading' || state.kind === 'installing') return;
      set({ kind: 'checking' });
      try {
        const update = await api.check();
        set(update ? { kind: 'available', update } : { kind: 'current' });
      } catch (error) { fail(error); }
    },
    async download() {
      if (state.kind !== 'available') return;
      const update = state.update;
      set({ kind: 'downloading', update, progress: { downloaded: 0, total: null } });
      try {
        await api.download((progress) => set({ kind: 'downloading', update, progress }));
        set({ kind: 'ready', update });
      } catch (error) { fail(error); }
    },
    async install() {
      if (state.kind !== 'ready') return;
      const update = state.update;
      set({ kind: 'installing', update });
      try {
        await api.install();
      } catch (error) {
        // A transfer guard or launcher failure leaves the verified bytes available for retry.
        set({ kind: 'ready', update });
        throw error;
      }
    },
  };
}
