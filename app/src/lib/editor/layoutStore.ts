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
  starterLayout,
  type Edit,
  type Layout,
  type Orientation,
} from './validate.ts';

import { friendlyFileError, friendlyRefusal, plural } from './messages.ts';

/**
 * What the editor last did, in plain words. `detail` keeps the technical reason, such as the rule a change broke or
 * the deck's reply, and `event` names the outcome for anything that needs to tell them apart.
 */
export interface Status {
  kind: 'info' | 'ok' | 'error';
  text: string;
  detail?: string;
  event?: string;
  pages?: number;
  tiles?: number;
}

interface LayoutState {
  source: string;
  name: string;
  orientation: Orientation;
  pages: number;
  tiles: number;
  error?: string | null;
}

interface PutReport {
  state: LayoutState;
  bytes: number;
  chunks: number;
  retransmits: number;
  elapsed_ms: number;
  icons: { sent: string[]; unavailable: string[] };
}

/** One layout per orientation plus a selection. Every layout it holds is valid. */
interface EditorState {
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
  return `${plural(pages, 'page')}, ${plural(tiles, 'button')}`;
};

function createEditor() {
  const store = writable<EditorState>(initial());
  const state = () => get(store);
  const current = () => state().layouts[state().orientation];
  const say = (kind: Status['kind'], text: string, extra: Omit<Status, 'kind' | 'text'> = {}) =>
    store.update((s) => ({ ...s, status: { kind, text, ...extra } }));

  function load(layout: Layout, note: string, event: string) {
    store.update((s) => ({
      ...s,
      layouts: { ...s.layouts, [layout.orientation]: layout },
      orientation: layout.orientation,
      page: layout.pages[0]?.id ?? null,
      selected: null,
      pulled: null,
      status: { kind: 'ok', text: note, event, ...counts(layout) },
    }));
  }

  /** Applies one edit to the current layout, or shows why it refused and keeps the layout. */
  function apply(edit: (layout: Layout) => Edit, select?: { page?: string | null; tile?: string | null }): string | null {
    const layout = current();
    if (!layout) return 'no layout is open';
    const result = edit(layout);
    if (!result.ok) {
      say('error', friendlyRefusal(result.reason, layout), { detail: result.reason, event: 'refused' });
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
      load(newLayout(state().orientation), 'Started a blank layout with one empty page.', 'new');
    },

    startStarter() {
      load(starterLayout(state().orientation), 'Started with media controls and everyday shortcuts. Change anything you like.', 'starter');
    },

    /** Reads a layout file. A file that breaks a rule leaves the editor as it was. */
    importText(text: string, name = 'file'): boolean {
      const parsed = parseLayout(text);
      if (!parsed.ok) {
        const [first] = parsed.errors;
        say('error', friendlyFileError(name, first.key), { detail: `${first.key}: ${first.message}`, event: 'open-failed' });
        return false;
      }
      load(parsed.layout, `Opened “${parsed.layout.name}”: ${describe(parsed.layout)}.`, 'opened');
      return true;
    },

    exportText(): string | null {
      const layout = current();
      return layout ? `${JSON.stringify(layout, null, 2)}\n` : null;
    },

    addTile(at?: [number, number]) {
      const { page } = state();
      const layout = current();
      if (!layout || !page) return;
      const cell = at ?? freeCell(layout, page);
      if (!cell) {
        say('error', friendlyRefusal('no free cell on this page'), { detail: 'no free cell on this page', event: 'refused' });
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
      store.update((s) => ({ ...s, busy: 'push', status: { kind: 'info', text: `Sending “${layout.name}” to your deck…`, event: 'pushing' } }));
      performance.mark('push-start');
      try {
        const report = await invoke<PutReport>('push_layout', { layout: canonical(layout) });
        performance.measure('push', 'push-start');
        const { state: deck } = report;
        const { sent, unavailable } = report.icons;
        const missing = unavailable.length ? `, ${plural(unavailable.length, 'icon')} not in the cache` : '';
        const detail = `Deck shows ${deck.name}: ${deck.pages} pages, ${deck.tiles} tiles (${report.bytes} bytes in ${report.elapsed_ms} ms), ${plural(sent.length, 'icon')} sent${missing}`;
        const shows = `Sent to your deck. It now shows “${deck.name}”: ${plural(deck.pages, 'page')}, ${plural(deck.tiles, 'button')}.`;
        const lost = unavailable.length === 1 ? 'that button shows only its name' : 'those buttons show only their names';
        say(
          unavailable.length ? 'error' : 'ok',
          unavailable.length ? `${shows} ${plural(unavailable.length, 'icon')} couldn't be found on this computer, so ${lost}.` : shows,
          { detail, event: 'pushed', pages: deck.pages, tiles: deck.tiles },
        );
        return report;
      } catch (error) {
        say('error', "Couldn't send to your deck. Check that it's plugged in and connected, then try again.", { detail: String(error), event: 'push-failed' });
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
      say('info', 'Clearing icons your deck no longer uses…', { event: 'icons-removing' });
      try {
        const removed = await invoke<string[]>('remove_unused_icons', { layouts });
        say('ok', `Removed ${plural(removed.length, 'unused icon')} from your deck.`, { detail: removed.join(', ') || 'none', event: 'icons-removed' });
        return removed;
      } catch (error) {
        say('error', "Couldn't clear unused icons. Check that your deck is connected, then try again.", { detail: String(error), event: 'icons-failed' });
        return null;
      }
    },

    /** Fetches the deck's layout for the current orientation and waits for a confirm. */
    async pull() {
      const { orientation } = state();
      store.update((s) => ({ ...s, busy: 'pull', status: { kind: 'info', text: `Loading the ${orientation} layout from your deck…`, event: 'pulling' } }));
      try {
        const text = await invoke<string>('pull_layout', { orientation });
        const parsed = parseLayout(text);
        if (!parsed.ok) {
          const [first] = parsed.errors;
          say('error', "Your deck's layout has a problem this app can't open.", { detail: `${first.key}: ${first.message}`, event: 'pull-failed' });
          return;
        }
        store.update((s) => ({
          ...s,
          pulled: { orientation, layout: parsed.layout },
          status: { kind: 'info', text: `Your deck has “${parsed.layout.name}”: ${describe(parsed.layout)}.`, event: 'pull-ready', ...counts(parsed.layout) },
        }));
      } catch (error) {
        say('error', "Couldn't load from your deck. Check that it's plugged in and connected, then try again.", { detail: String(error), event: 'pull-failed' });
      } finally {
        store.update((s) => ({ ...s, busy: null }));
      }
    },

    confirmPull() {
      const pulled = state().pulled;
      if (pulled) load(pulled.layout, `Loaded “${pulled.layout.name}” from your deck: ${describe(pulled.layout)}.`, 'pulled');
    },

    cancelPull() {
      store.update((s) => ({ ...s, pulled: null, status: { kind: 'info', text: 'Kept your layout. Nothing changed.', event: 'pull-cancelled' } }));
    },
  };
}

export const editor = createEditor();
