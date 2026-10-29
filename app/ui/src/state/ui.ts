import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { Appearance } from './theme.ts';

/** The screens the nav cluster switches between. Settings is a dialog over them. */
export type Section = 'home' | 'layout' | 'integrations';

export interface UiState {
  /** Home is where the app always starts. */
  section: Section;
  settingsOpen: boolean;
  deviceId: string | null;
  settingsSection: 'general' | 'troubleshooting';
  openDevice(id: string): void;
  openTroubleshooting(): void;
  /** Kept in memory for now; Settings owns where it's saved (#88). */
  appearance: Appearance;
  go(section: Section): void;
  setSettingsOpen(open: boolean): void;
  setAppearance(appearance: Appearance): void;
}

export function createUiStore() {
  return createStore<UiState>()((set) => ({
    section: 'home',
    deviceId: null,
    settingsSection: 'general',
    openDevice: (deviceId) => set({ section: 'layout', deviceId }),
    openTroubleshooting: () => set({ settingsOpen: true, settingsSection: 'troubleshooting' }),
    settingsOpen: false,
    appearance: 'system',
    go: (section) => set({ section }),
    setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
    setAppearance: (appearance) => set({ appearance }),
  }));
}

export const uiStore = createUiStore();

export function useUi<T>(select: (state: UiState) => T): T {
  return useStore(uiStore, select);
}
