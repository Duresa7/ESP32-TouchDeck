import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { IntegrationSummary } from './api.ts';

/** Every Integration the Companion runs: the `integrations` key of `sync`, then `integration.status`. */
export interface IntegrationsState {
  integrations: IntegrationSummary[];
  sync(integrations: IntegrationSummary[]): void;
  /** A full summary replaces the one with its id. */
  upsert(summary: IntegrationSummary): void;
}

export function createIntegrationsStore() {
  return createStore<IntegrationsState>()((set) => ({
    integrations: [],
    sync: (integrations) => set({ integrations }),
    upsert: (summary) =>
      set((state) => {
        const at = state.integrations.findIndex((i) => i.id === summary.id);
        return { integrations: at < 0 ? [...state.integrations, summary] : state.integrations.with(at, summary) };
      }),
  }));
}

export type IntegrationsStore = ReturnType<typeof createIntegrationsStore>;

export const integrationsStore = createIntegrationsStore();

export function useIntegrations<T>(select: (state: IntegrationsState) => T): T {
  return useStore(integrationsStore, select);
}
