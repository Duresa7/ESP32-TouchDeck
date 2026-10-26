import { invoke } from '@tauri-apps/api/core';
import { get, writable } from 'svelte/store';
import {
  addPage,
  addTile,
  canonical,
  counts,
  freeCell,
  freshId,
  newGridPage,
  newLayout,
  newTile,
  parseLayout,
  removePage,
  removeTile,
  type Edit,
  type Layout,
  type Orientation,
} from './validate.ts';

export interface Status {
  kind: 'info' | 'ok' | 'error';
  text: string;
}

export interface LayoutState {
  source: string;
  name: string;
  orientation: Orientation;
  pages: number;
  tiles: number;
  error?: string | null;
}

export interface PutReport {
  state: LayoutState;
  bytes: number;
  chunks: number;
  retransmits: number;
  elapsed_ms: number;
  icons: { sent: string[]; unavailable: string[] };
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

/** One layout per orientation plus a selection. Every layout it holds is valid. */
export interface EditorState {
  layouts: Record<Orientation, Layout | null>;
  orientation: Orientation;
  page: string | null;
  selected: string | null;
  status: Status | null;
  pulled: { orientation: Orientation; layout: Layout } | null;
  busy: 'push' | 'pull' | null;
}

const initial = (): EditorState => ({
  layouts: { landscape: null, portrait: null },
  orientation: 'landscape',
  page: null,
  selected: null,
  status: null,
  pulled: null,
  busy: null,
});

const describe = (layout: Layout) => {
  const { pages, tiles } = counts(layout);
  return `${pages} pages, ${tiles} tiles`;
};

export function createEditor() {
  const store = writable<EditorState>(initial());
  const state = () => get(store);
  const current = () => state().layouts[state().orientation];
  const say = (kind: Status['kind'], text: string) => store.update((s) => ({ ...s, status: { kind, text } }));

  function load(layout: Layout, note: string) {
    store.update((s) => ({
      ...s,
      layouts: { ...s.layouts, [layout.orientation]: layout },
      orientation: layout.orientation,
      page: layout.pages[0]?.id ?? null,
      selected: null,
      pulled: null,
      status: { kind: 'ok', text: note },
    }));
  }

  /** Applies one edit to the current layout, or shows why it refused and keeps the layout. */
  function apply(edit: (layout: Layout) => Edit, select?: { page?: string | null; tile?: string | null }): string | null {
    const layout = current();
    if (!layout) return 'no layout is open';
    const result = edit(layout);
    if (!result.ok) {
      say('error', result.reason);
      return result.reason;
    }
    store.update((s) => {
      const page = select?.page !== undefined ? select.page : s.page;
      const pageExists = result.layout.pages.some((p) => p.id === page);
      return {
        ...s,
        layouts: { ...s.layouts, [s.orientation]: result.layout },
        page: pageExists ? page : (result.layout.pages[0]?.id ?? null),
        selected: select?.tile !== undefined ? select.tile : s.selected,
        status: null,
      };
    });
    return null;
  }

  return {
    subscribe: store.subscribe,
    apply,
    say,

    setOrientation(orientation: Orientation) {
      store.update((s) => ({
        ...s,
        orientation,
        page: s.layouts[orientation]?.pages[0]?.id ?? null,
        selected: null,
        status: null,
      }));
    },

    selectPage(page: string) {
      store.update((s) => ({ ...s, page, selected: null }));
    },

    select(tile: string | null) {
      store.update((s) => ({ ...s, selected: tile }));
    },

    startNew() {
      load(newLayout(state().orientation), 'New layout with 1 page');
    },

    /** Reads a layout file. A file that breaks a rule leaves the editor as it was. */
    importText(text: string, name = 'file'): boolean {
      const parsed = parseLayout(text);
      if (!parsed.ok) {
        const [first] = parsed.errors;
        say('error', `${name} is not a valid layout: ${first.key}: ${first.message}`);
        return false;
      }
      load(parsed.layout, `Imported ${parsed.layout.name}: ${describe(parsed.layout)}`);
      return true;
    },

    exportText(): string | null {
      const layout = current();
      return layout ? `${JSON.stringify(layout, null, 2)}\n` : null;
    },

    addTile() {
      const { page } = state();
      const layout = current();
      if (!layout || !page) return;
      const cell = freeCell(layout, page);
      if (!cell) {
        say('error', 'no free cell on this page');
        return;
      }
      const id = freshId(layout, 'tile');
      apply((l) => addTile(l, page, newTile(id, cell[0], cell[1])), { tile: id });
    },

    removeTile(tile: string) {
      const { page } = state();
      if (page) apply((l) => removeTile(l, page, tile), { tile: null });
    },

    addPage() {
      const layout = current();
      if (!layout) return;
      const id = freshId(layout, 'page');
      apply((l) => addPage(l, newGridPage(id, `Page ${l.pages.length + 1}`)), { page: id, tile: null });
    },

    removePage(page: string) {
      apply((l) => removePage(l, page), { tile: null });
    },

    async push(): Promise<PutReport | null> {
      const layout = current();
      if (!layout) return null;
      store.update((s) => ({ ...s, busy: 'push', status: { kind: 'info', text: `Pushing ${layout.name}` } }));
      performance.mark('push-start');
      try {
        const report = await invoke<PutReport>('push_layout', { layout: canonical(layout) });
        performance.measure('push', 'push-start');
        const { state: deck } = report;
        const { sent, unavailable } = report.icons;
        const missing = unavailable.length ? `, ${plural(unavailable.length, 'icon')} not in the cache` : '';
        say(
          unavailable.length ? 'error' : 'ok',
          `Deck shows ${deck.name}: ${deck.pages} pages, ${deck.tiles} tiles (${report.bytes} bytes in ${report.elapsed_ms} ms), ${plural(sent.length, 'icon')} sent${missing}`,
        );
        return report;
      } catch (error) {
        say('error', `Push failed: ${String(error)}`);
        return null;
      } finally {
        store.update((s) => ({ ...s, busy: null }));
      }
    },

    /** Removes every icon on the deck that no layout in the editor or on the deck names. */
    async removeUnusedIcons(): Promise<string[] | null> {
      const layouts = Object.values(state().layouts)
        .filter((layout): layout is Layout => layout !== null)
        .map(canonical);
      say('info', 'Removing unused icons');
      try {
        const removed = await invoke<string[]>('remove_unused_icons', { layouts });
        say('ok', `${plural(removed.length, 'unused icon')} removed`);
        return removed;
      } catch (error) {
        say('error', `Remove unused icons failed: ${String(error)}`);
        return null;
      }
    },

    /** Fetches the deck's layout for the current orientation and waits for a confirm. */
    async pull() {
      const { orientation } = state();
      store.update((s) => ({ ...s, busy: 'pull', status: { kind: 'info', text: `Pulling the ${orientation} layout` } }));
      try {
        const text = await invoke<string>('pull_layout', { orientation });
        const parsed = parseLayout(text);
        if (!parsed.ok) {
          const [first] = parsed.errors;
          say('error', `The deck's layout is not valid: ${first.key}: ${first.message}`);
          return;
        }
        store.update((s) => ({
          ...s,
          pulled: { orientation, layout: parsed.layout },
          status: { kind: 'info', text: `The deck holds ${parsed.layout.name}: ${describe(parsed.layout)}` },
        }));
      } catch (error) {
        say('error', `Pull failed: ${String(error)}`);
      } finally {
        store.update((s) => ({ ...s, busy: null }));
      }
    },

    confirmPull() {
      const pulled = state().pulled;
      if (pulled) load(pulled.layout, `Pulled ${pulled.layout.name}: ${describe(pulled.layout)}`);
    },

    cancelPull() {
      store.update((s) => ({ ...s, pulled: null, status: { kind: 'info', text: 'Pull cancelled; the editor is unchanged' } }));
    },
  };
}

export const editor = createEditor();
