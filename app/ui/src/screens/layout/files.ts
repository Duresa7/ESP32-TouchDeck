// Import and Export for the Layout screen. Export asks where to save when the browser can, and downloads otherwise.
import type { EditorStore } from '../../lib/editor/store.ts';

type SavePicker = (options: object) => Promise<{ createWritable(): Promise<{ write(data: string): Promise<void>; close(): Promise<void> }> }>;

/** The file name Export suggests: the Layout's name with anything unsafe in a file name turned into dashes. */
export const exportName = (layout: string) => `${layout.trim().replace(/[^A-Za-z0-9._-]+/g, '-') || 'layout'}.json`;

export async function exportLayout(store: EditorStore) {
  const { drafts, orientation } = store.getState();
  const draft = drafts[orientation];
  const text = store.exportText();
  if (!draft || !text) return;
  const name = exportName(draft.layout.name);
  const save = (window as unknown as { showSaveFilePicker?: SavePicker }).showSaveFilePicker;
  try {
    if (save) {
      const handle = await save({ suggestedName: name, types: [{ description: 'Open TouchDeck Layout', accept: { 'application/json': ['.json'] } }] });
      const writable = await handle.createWritable();
      await writable.write(text);
      await writable.close();
    } else {
      const link = document.createElement('a');
      link.href = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
      link.download = name;
      link.click();
      URL.revokeObjectURL(link.href);
    }
    store.say('ok', `Exported “${name}”`, { event: 'exported' });
  } catch (error) {
    if ((error as DOMException).name !== 'AbortError') store.say('error', "Couldn't export. Pick another folder, then retry.", { detail: String(error), event: 'export-failed' });
  }
}

export async function importLayout(store: EditorStore, file: File) {
  let text: string;
  try {
    text = await file.text();
  } catch (error) {
    store.say('error', `“${file.name}” isn't a Layout file. Nothing changed.`, { detail: String(error), event: 'open-failed' });
    return;
  }
  store.importText(text, file.name);
}
