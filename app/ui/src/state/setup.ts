import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { DeviceSummary } from '../companion/api.ts';
import { readSkipped, writeSkipped, type SkipStorage } from '../lib/device/setup-trigger.ts';

export interface SetupUiState {
  open: string | null;
  skipped: ReadonlySet<string>;
  start(id: string): void;
  close(device?: Pick<DeviceSummary, 'id' | 'setup'>): void;
  unskip(id: string): void;
}

export function createSetupUi(storage: SkipStorage | null) {
  return createStore<SetupUiState>()((set, get) => ({
    open: null,
    skipped: readSkipped(storage),
    start: (id) => set({ open: id }),
    close: (device) => {
      if (device?.setup === 'needs-setup' && !get().skipped.has(device.id)) {
        const skipped = new Set(get().skipped).add(device.id);
        writeSkipped(storage, skipped);
        set({ skipped });
      }
      set({ open: null });
    },
    unskip: (id) => {
      if (!get().skipped.has(id)) return;
      const skipped = new Set(get().skipped);
      skipped.delete(id);
      writeSkipped(storage, skipped);
      set({ skipped });
    },
  }));
}

function localStorageOrNull() {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export const setupUi = createSetupUi(typeof window === 'undefined' ? null : localStorageOrNull());

export function useSetupUi<T>(select: (state: SetupUiState) => T): T {
  return useStore(setupUi, select);
}
