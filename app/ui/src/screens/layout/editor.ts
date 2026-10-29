import { useStore } from 'zustand';
import type { Companion } from '../../companion/client.ts';
import { jobsStore, waitForJob } from '../../companion/jobs.ts';
import { createEditorStore, type EditorState, type EditorStore } from '../../lib/editor/store.ts';

const stores = new WeakMap<Companion, EditorStore>();

function storage() {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/** The one editor store for a Companion, so a draft and its selection outlive switching screens. */
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
