import type { EditorStore } from '../../lib/editor/store.ts';

declare global {
  interface Window {
    showSaveFilePicker?(options: { suggestedName: string; types: { description: string; accept: Record<string, string[]> }[] }): Promise<FileSystemFileHandle>;
  }
}

export const exportName = (layout: string) => `${layout.trim().replace(/[^A-Za-z0-9._-]+/g, '-') || 'layout'}.json`;

export async function exportLayout(store: EditorStore) {
  const { drafts, orientation } = store.getState();
  const draft = drafts[orientation];
  const text = store.exportText();
  if (!draft || !text) return;
  const name = exportName(draft.layout.name);
  const save = window.showSaveFilePicker;
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
    if (!(error instanceof DOMException && error.name === 'AbortError')) store.say('error', "Couldn't export. Pick another folder, then retry.", { detail: String(error), event: 'export-failed' });
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
