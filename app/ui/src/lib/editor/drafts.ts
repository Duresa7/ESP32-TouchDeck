import { canonical, parseLayout, type Layout, type Orientation } from './validate.ts';

export interface Draft {
  layout: Layout;
  onDevice: string | null;
  revision: string | null;
}

export type DraftStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

export const draftKey = (device: string, orientation: Orientation) => `touchdeck.layout-draft.${device}.${orientation}`;

interface Stored {
  v: 1;
  layout: string;
  sent: string | null;
  revision: string | null;
}

const textOrNull = (value: unknown) => (typeof value === 'string' ? value : null);

function parseStored(text: string): Stored | null {
  const value: unknown = JSON.parse(text);
  if (typeof value !== 'object' || value === null || !('v' in value) || value.v !== 1 || !('layout' in value) || typeof value.layout !== 'string') return null;
  return {
    v: 1,
    layout: value.layout,
    sent: 'sent' in value ? textOrNull(value.sent) : null,
    revision: 'revision' in value ? textOrNull(value.revision) : null,
  };
}

export function readDraft(storage: DraftStorage, device: string, orientation: Orientation): Draft | null {
  let stored: Stored | null;
  try {
    const text = storage.getItem(draftKey(device, orientation));
    if (text === null) return null;
    stored = parseStored(text);
  } catch {
    return null;
  }
  if (stored === null) return null;
  const parsed = parseLayout(stored.layout);
  if (!parsed.ok || parsed.layout.orientation !== orientation) return null;
  return { layout: parsed.layout, onDevice: stored.sent, revision: stored.revision };
}

export function writeDraft(storage: DraftStorage, device: string, orientation: Orientation, draft: Draft | null) {
  const key = draftKey(device, orientation);
  try {
    if (draft === null) storage.removeItem(key);
    else storage.setItem(key, JSON.stringify({ v: 1, layout: canonical(draft.layout), sent: draft.onDevice, revision: draft.revision } satisfies Stored));
  } catch {}
}

export const isSent = (draft: Draft) => draft.onDevice !== null && draft.onDevice === canonical(draft.layout);
