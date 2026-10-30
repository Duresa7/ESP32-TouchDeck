import { useStore } from 'zustand';
import type { Companion } from '../../companion/client.ts';
import { useDevices } from '../../companion/devices.ts';
import { jobsStore, waitForJob } from '../../companion/jobs.ts';
import { showsKind, type TileKind } from '../../lib/device/needs-firmware.ts';
import { createEditorStore, type EditorState, type EditorStore } from '../../lib/editor/store.ts';

const stores = new WeakMap<Companion, EditorStore>();

function storage() {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function editorFor(companion: Companion): EditorStore {
  let store = stores.get(companion);
  if (!store) {
    store = createEditorStore({ companion, storage: storage(), wait: (job) => waitForJob(jobsStore, companion, job) });
    stores.set(companion, store);
  }
  return store;
}

export function useEditorState<T>(store: EditorStore, select: (state: EditorState) => T): T {
  return useStore(store, select);
}

/** Whether the open Device's Firmware draws this kind of Tile. */
export function useDeviceShows(store: EditorStore, kind: TileKind): boolean {
  const id = useEditorState(store, (s) => s.device?.id);
  return useDevices((s) => {
    const device = s.devices.find((d) => d.id === id);
    return device ? showsKind(device, kind) : false;
  });
}
