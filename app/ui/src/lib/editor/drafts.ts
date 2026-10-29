// Unsent Layouts, one per Device and orientation. The Companion has no draft store, so they live in the UI's own
// storage, keyed by Device id. Storage belongs to the page's origin: a Companion that comes back on another port
// starts with no drafts.
import { canonical, parseLayout, type Layout, type Orientation } from './validate.ts';

export interface Draft {
  layout: Layout;
  /** The Layout as the Device last had it, sent or loaded, in canonical form. Null when unknown. */
  sent: string | null;
  /** The Device's revision for `sent`, the next `If-Match`. Null when unknown. */
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

/** The saved draft, or null when there's none or it no longer passes the Layout rules. */
export function readDraft(storage: DraftStorage, device: string, orientation: Orientation): Draft | null {
  let stored: Stored;
  try {
    const text = storage.getItem(draftKey(device, orientation));
    if (text === null) return null;
    stored = JSON.parse(text) as Stored;
  } catch {
    return null;
  }
  if (stored?.v !== 1 || typeof stored.layout !== 'string') return null;
  const parsed = parseLayout(stored.layout);
  if (!parsed.ok || parsed.layout.orientation !== orientation) return null;
  return {
    layout: parsed.layout,
    sent: typeof stored.sent === 'string' ? stored.sent : null,
    revision: typeof stored.revision === 'string' ? stored.revision : null,
  };
}

export function writeDraft(storage: DraftStorage, device: string, orientation: Orientation, draft: Draft | null) {
  const key = draftKey(device, orientation);
  try {
    if (draft === null) storage.removeItem(key);
    else storage.setItem(key, JSON.stringify({ v: 1, layout: canonical(draft.layout), sent: draft.sent, revision: draft.revision } satisfies Stored));
  } catch {
    // A full or blocked storage keeps the draft in memory only.
  }
}

/** Whether the Device has exactly this draft, as far as the editor knows. */
export const isSent = (draft: Draft) => draft.sent !== null && draft.sent === canonical(draft.layout);
