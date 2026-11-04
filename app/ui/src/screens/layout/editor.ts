import { useStore } from 'zustand';
import type { Companion } from '../../companion/client.ts';
import { useCompanion } from '../../companion/connection.ts';
import { devicesStore, useDevices } from '../../companion/devices.ts';
import { jobsStore, waitForJob } from '../../companion/jobs.ts';
import { showsKind, type TileKind } from '../../lib/device/needs-firmware.ts';
import { offersGif } from '../../lib/editor/gif.ts';
import { createGifTiles, type GifTiles, type GifTilesState } from '../../lib/editor/gif-tiles.ts';
import { controlsStore, type ControlsStore } from '../../lib/device/controls.ts';
import { createEditorStore, type EditorDevice, type EditorState, type EditorStore } from '../../lib/editor/store.ts';
import type { Orientation } from '../../lib/editor/validate.ts';

const stores = new WeakMap<Companion, EditorStore>();
const gifStores = new WeakMap<Companion, GifTiles>();

function storage() {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function watchSends(editor: EditorStore, controls: ControlsStore): () => void {
  return editor.subscribe((next, previous) => {
    const device = next.device?.id;
    if (!device || next.busy === previous.busy) return;
    if (next.busy === 'push') controls.getState().sending(device, next.orientation);
    else if (previous.busy === 'push') controls.getState().sent(device, next.status?.event === 'pushed' ? 'succeeded' : 'failed');
  });
}

export function editorFor(companion: Companion): EditorStore {
  let store = stores.get(companion);
  if (!store) {
    store = createEditorStore({ companion, storage: storage(), wait: (job) => waitForJob(jobsStore, companion, job) });
    watchSends(store, controlsStore);
    stores.set(companion, store);
  }
  return store;
}

export function reviewLayout(store: EditorStore, device: EditorDevice, orientation: Orientation) {
  store.open(device, orientation);
  store.setOrientation(orientation);
  void store.load();
}

/** The GIF Tiles of the editor that `editorFor` gives for this Companion. */
export function gifTilesFor(companion: Companion): GifTiles {
  let gifs = gifStores.get(companion);
  if (!gifs) {
    gifs = createGifTiles({
      editor: editorFor(companion),
      companion,
      jobs: jobsStore,
      devices: devicesStore,
      wait: (job, signal) => waitForJob<'assets.render'>(jobsStore, companion, job, { signal }),
    });
    gifStores.set(companion, gifs);
  }
  return gifs;
}

export function useGifTiles(): GifTiles {
  return gifTilesFor(useCompanion());
}

export function useGifState<T>(gifs: GifTiles, select: (state: GifTilesState) => T): T {
  return useStore(gifs, select);
}

/** Whether the open Device takes animated GIF Tiles. */
export function useDeviceOffersGif(store: EditorStore): boolean {
  const id = useEditorState(store, (s) => s.device?.id);
  return useDevices((s) => offersGif(s.devices.find((d) => d.id === id)?.capabilities));
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
