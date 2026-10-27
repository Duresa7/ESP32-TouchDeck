import { Channel, invoke } from '@tauri-apps/api/core';
import { writable } from 'svelte/store';
import { createAppUpdater, type Progress, type UpdateInfo, type UpdateState } from './controller.ts';

export const appUpdateState = writable<UpdateState>({ kind: 'idle' });
export const appUpdater = createAppUpdater({
  changed: (state) => appUpdateState.set(state),
  api: {
    check: () => invoke<UpdateInfo | null>('app_update_check'),
    download: async (progress) => {
      const channel = new Channel<Progress>();
      channel.onmessage = progress;
      await invoke('app_update_download', { progress: channel });
    },
    install: () => invoke('app_update_install'),
  },
});
