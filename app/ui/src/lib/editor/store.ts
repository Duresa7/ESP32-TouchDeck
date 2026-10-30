import { createStore } from 'zustand/vanilla';
import type { Job, JobMap, PresetTile } from '../../companion/api.ts';
import type { Companion } from '../../companion/client.ts';
import { toApiError, type ApiError } from '../../companion/errors.ts';
import { parseAction, presetLabel } from '../integrations/model.ts';
import { isSent, readDraft, writeDraft, type Draft, type DraftStorage } from './drafts.ts';
import { freshAction } from './actions.ts';
import { describeLayout, friendlyFileError, friendlyRefusal, plural, PLUG_IN } from './messages.ts';
import {
  addPage,
  addTile,
  canonical,
  counts,
  freeCell,
  freshId,
  graphTile,
  moveTile,
  newGridPage,
  newLayout,
  parseLayout,
  removePage,
  removeTile,
  resizeTile,
  assetOf,
  setAction,
  setAsset,
  setFullScreen,
  setGraph,
  setGridSize,
  setIcon,
  setLabel,
  setLayoutName,
  setPageTitle,
  starterLayout,
  type Action,
  type Edit,
  type GraphBinding,
  type GridPage,
  type Layout,
  type Orientation,
  type Tile,
} from './validate.ts';

export interface Status {
  kind: 'info' | 'ok' | 'error';
  text: string;
  detail?: string;
  event: string;
  pages?: number;
  tiles?: number;
}

export type Target = { row: number; col: number } | 'new-page';

export interface Conflict {
  reason: 'load' | 'changed';
  orientation: Orientation;
  layout: Layout;
  revision: string;
}

export type SaveState = 'none' | 'saved' | 'unsent' | 'sending';

export interface Refused<T> {
  value: T;
  reason: string;
}

export interface TileProblems {
  label?: Refused<string>;
  action?: Refused<Action>;
}

export interface EditorDevice {
  id: string;
  name: string;
}

export interface EditorState {
  device: EditorDevice | null;
  orientation: Orientation;
  drafts: Record<Orientation, Draft | null>;
  page: string | null;
  selected: string | null;
  adding: Target | null;
  mode: 'edit' | 'preview';
  status: Status | null;
  busy: 'push' | 'load' | 'prune' | null;
  conflict: Conflict | null;
  invalid: Record<Orientation, Record<string, TileProblems>>;
}

export const ADD_KINDS = ['key', 'launch', 'consumer', 'page', 'mod', 'open', 'command', 'none'] as const;
export type AddKind = (typeof ADD_KINDS)[number];

export function defaultAction(kind: AddKind, pages: string[]): Action | null {
  switch (kind) {
    case 'key':
      return { type: 'key', usage: 58, mods: [] };
    case 'consumer':
      return { type: 'consumer', usage: 205 };
    case 'mod':
      return { type: 'mod', mod: 'ctrl' };
    case 'page':
      return { type: 'page', page: pages[1] ?? pages[0] };
    case 'none':
      return { type: 'none' };
    default:
      return null;
  }
}

export interface EditorDeps {
  companion: Pick<Companion, 'layouts' | 'assets'>;
  wait<K extends keyof JobMap>(job: Job): Promise<JobMap[K]['result']>;
  storage: DraftStorage | null;
}

const initial = (): Omit<EditorState, 'device'> => ({
  orientation: 'landscape',
  drafts: { landscape: null, portrait: null },
  page: null,
  selected: null,
  adding: null,
  mode: 'edit',
  status: null,
  busy: null,
  conflict: null,
  invalid: { landscape: {}, portrait: {} },
});

const gridOf = (layout: Layout | undefined, page: string | null): GridPage | null => {
  const found = layout?.pages.find((p) => p.id === page);
  return found?.kind === 'grid' ? found : null;
};

const covers = (tile: Tile, row: number, col: number) =>
  row >= tile.row && row < tile.row + tile.rowSpan && col >= tile.col && col < tile.col + tile.colSpan;

const tileIds = (layout: Layout) => new Set(layout.pages.flatMap((page) => (page.kind === 'grid' ? page.tiles.map((tile) => tile.id) : [])));

export const tileProblems = (state: EditorState, tile: string): TileProblems | undefined => state.invalid[state.orientation][tile];

export function firstProblem(state: EditorState): { page: string; tile: string; reason: string } | null {
  const layout = state.drafts[state.orientation]?.layout;
  for (const page of layout?.pages ?? []) {
    if (page.kind !== 'grid') continue;
    for (const tile of page.tiles) {
      const problems = state.invalid[state.orientation][tile.id];
      const reason = problems?.label?.reason ?? problems?.action?.reason;
      if (reason) return { page: page.id, tile: tile.id, reason };
    }
  }
  return null;
}

export function assetIds(layout: Layout): string[] {
  const ids = new Set<string>();
  for (const page of layout.pages) {
    if (page.kind !== 'grid') continue;
    for (const tile of page.tiles) {
      if (tile.icon?.startsWith('file:')) ids.add(tile.icon.slice(5));
      const asset = assetOf(tile);
      if (asset) ids.add(asset);
    }
  }
  return [...ids];
}

export function createEditorStore(deps: EditorDeps) {
  const store = createStore<EditorState>()(() => ({ device: null, ...initial() }));
  const { getState: state, setState: set } = store;

  const name = () => state().device?.name ?? 'the Device';
  const current = () => state().drafts[state().orientation];
  const say = (kind: Status['kind'], text: string, extra: Omit<Status, 'kind' | 'text'>) => set({ status: { kind, text, ...extra } });

  function keep(orientation: Orientation, draft: Draft | null) {
    const device = state().device;
    set((s) => {
      const ids = draft ? tileIds(draft.layout) : new Set<string>();
      const problems = Object.entries(s.invalid[orientation]).filter(([id]) => ids.has(id));
      return { drafts: { ...s.drafts, [orientation]: draft }, invalid: { ...s.invalid, [orientation]: Object.fromEntries(problems) } };
    });
    if (device && deps.storage) writeDraft(deps.storage, device.id, orientation, draft);
  }

  function mark(tile: string, change: TileProblems) {
    set((s) => {
      const orientation = s.orientation;
      const next = { ...s.invalid[orientation][tile], ...change };
      const { [tile]: _was, ...others } = s.invalid[orientation];
      const tiles = next.label || next.action ? { ...others, [tile]: next } : others;
      return { invalid: { ...s.invalid, [orientation]: tiles } };
    });
  }

  function editField(tile: string, edit: (layout: Layout, page: string) => Edit, refused: (reason: string) => TileProblems, field: keyof TileProblems): string | null {
    const draft = current();
    const page = state().page;
    if (!draft || !page) return 'no layout is open';
    const result = edit(draft.layout, page);
    if (!result.ok) {
      mark(tile, refused(result.reason));
      return result.reason;
    }
    keep(state().orientation, { ...draft, layout: result.layout });
    mark(tile, { [field]: undefined });
    return null;
  }

  function start(layout: Layout, status: Status, known?: { onDevice: string | null; revision: string | null }) {
    const before = state().drafts[layout.orientation];
    set((s) => ({ invalid: { ...s.invalid, [layout.orientation]: {} } }));
    keep(layout.orientation, { layout, onDevice: known?.onDevice ?? before?.onDevice ?? null, revision: known?.revision ?? before?.revision ?? null });
    set({
      orientation: layout.orientation,
      page: layout.pages[0]?.id ?? null,
      selected: null,
      adding: null,
      conflict: null,
      status,
    });
  }

  function apply(edit: (layout: Layout) => Edit, next: Partial<Pick<EditorState, 'page' | 'selected' | 'adding'>> = {}): string | null {
    const draft = current();
    if (!draft) return 'no layout is open';
    const result = edit(draft.layout);
    if (!result.ok) {
      say('error', friendlyRefusal(result.reason, draft.layout, name()), { detail: result.reason, event: 'refused' });
      return result.reason;
    }
    keep(state().orientation, { ...draft, layout: result.layout });
    set((s) => {
      const page = next.page !== undefined ? next.page : s.page;
      return {
        page: result.layout.pages.some((p) => p.id === page) ? page : (result.layout.pages[0]?.id ?? null),
        selected: next.selected !== undefined ? next.selected : s.selected,
        adding: next.adding !== undefined ? next.adding : s.adding,
        status: null,
      };
    });
    return null;
  }

  function target(): Target {
    const draft = current();
    const page = state().page;
    const grid = gridOf(draft?.layout, page);
    const cell = grid && draft ? freeCell(draft.layout, grid.id) : null;
    return cell ? { row: cell[0], col: cell[1] } : 'new-page';
  }

  async function readLayout(): Promise<{ layout: Layout; revision: string } | { problem: string }> {
    const { device, orientation } = state();
    const read = await deps.companion.layouts.get({ device: device!.id, orientation });
    const parsed = parseLayout(JSON.stringify(read.layout));
    if (!parsed.ok) {
      const [first] = parsed.errors;
      return { problem: `${first.key}: ${first.message}` };
    }
    return { layout: parsed.layout, revision: read.revision };
  }

  function refusals(details: unknown): string[] {
    if (typeof details !== 'object' || details === null || !('issues' in details) || !Array.isArray(details.issues)) return [];
    return details.issues.map((issue: unknown) => {
      const known = typeof issue === 'object' && issue !== null ? issue : {};
      const rule = 'rule' in known && typeof known.rule === 'string' ? known.rule : 'layout.invalid';
      const message = 'message' in known && typeof known.message === 'string' ? known.message : '';
      return `${rule}: ${message}`;
    });
  }

  function sendFailed(error: ApiError, layout: Layout) {
    const device = name();
    if (error.code === 'layout.invalid') {
      const issues = refusals(error.details);
      const text = issues.length ? friendlyRefusal(issues[0], layout, device) : 'That change breaks a Layout rule. See Details.';
      say('error', text, { detail: issues.join('\n') || error.message, event: 'push-failed' });
    } else if (error.code === 'device.busy') {
      say('error', `${device} is busy. Wait a moment, then retry.`, { detail: error.message, event: 'push-failed' });
    } else if (error.code === 'job.cancelled') {
      say('error', `Sending stopped. ${device} keeps its Layout.`, { detail: error.message, event: 'push-failed' });
    } else {
      say('error', `Couldn't send to ${device}. ${PLUG_IN}`, { detail: error.message, event: 'push-failed' });
    }
  }

  async function send(layout: Layout, revision: string | null): Promise<void> {
    const { device, orientation } = state();
    const job = await deps.companion.layouts.push(
      { device: device!.id, orientation },
      null,
      layout,
      undefined,
      revision ? { 'If-Match': revision } : undefined,
    );
    const report = await deps.wait<'layouts.push'>(job);
    const sentDraft = state().drafts[orientation];
    if (sentDraft) keep(orientation, { ...sentDraft, onDevice: canonical(layout), revision: report.revision });
    const { sent, unavailable } = report.assets;
    const shown = report.state;
    const missing = unavailable.length ? `, ${plural(unavailable.length, 'icon')} not cached` : '';
    say(
      unavailable.length ? 'error' : 'ok',
      unavailable.length
        ? `Sent to ${name()}, but ${plural(unavailable.length, 'icon is', 'icons are')} missing on this PC, so those Tiles show only their names.`
        : `Sent to ${name()}`,
      {
        detail: `${name()} shows ${shown.name}: ${plural(shown.pages, 'Page')}, ${plural(shown.tiles, 'Tile')} (${report.bytes} bytes in ${report.elapsed_ms} ms), ${plural(sent.length, 'icon')} sent${missing}`,
        event: 'pushed',
        pages: shown.pages,
        tiles: shown.tiles,
      },
    );
  }

  function editLabel(tile: string, label: string): string | null {
    return editField(tile, (l, page) => setLabel(l, page, tile, label), (reason) => ({ label: { value: label, reason } }), 'label');
  }

  function editAction(tile: string, action: Action): string | null {
    return editField(tile, (l, page) => setAction(l, page, tile, action), (reason) => ({ action: { value: action, reason } }), 'action');
  }

  function placeAtTarget(action: Action, label: string, graph: GraphBinding | null = null): { id: string; page: string; pages: string[] } | null {
    const draft = current();
    const where = state().adding;
    if (!draft || !where) return null;
    let page = state().page;
    let layout = draft.layout;
    let cell = where === 'new-page' ? { row: 0, col: 0 } : where;
    if (where === 'new-page') {
      const id = freshId(layout, 'page');
      const added = addPage(layout, newGridPage(id, `Page ${layout.pages.length + 1}`));
      if (!added.ok) {
        say('error', friendlyRefusal(added.reason, layout, name()), { detail: added.reason, event: 'refused' });
        return null;
      }
      layout = added.layout;
      page = id;
      cell = { row: 0, col: 0 };
    }
    const target = page;
    if (!target) return null;
    const id = freshId(layout, 'tile');
    const plain: Tile = { id, label, row: cell.row, col: cell.col, rowSpan: 1, colSpan: 1, icon: null, action };
    const tile = graph ? graphTile(plain, graph) : plain;
    const base: Layout = graph ? { ...layout, schema: 2 } : layout;
    if (apply(() => addTile(base, target, tile), { page: target, selected: id, adding: null }) !== null) return null;
    return { id, page: target, pages: base.pages.map((p) => p.id) };
  }

  const api = Object.assign(store, {
    open(device: EditorDevice, orientation: Orientation = 'landscape') {
      if (state().device?.id === device.id) {
        if (state().device!.name !== device.name) set({ device });
        return;
      }
      const drafts = {
        landscape: deps.storage ? readDraft(deps.storage, device.id, 'landscape') : null,
        portrait: deps.storage ? readDraft(deps.storage, device.id, 'portrait') : null,
      };
      set({ device, ...initial(), orientation, drafts, page: drafts[orientation]?.layout.pages[0]?.id ?? null });
    },

    setOrientation(orientation: Orientation) {
      if (orientation === state().orientation) return;
      set((s) => ({
        orientation,
        page: s.drafts[orientation]?.layout.pages[0]?.id ?? null,
        selected: null,
        adding: null,
        status: null,
      }));
    },

    setMode(mode: EditorState['mode']) {
      set({ mode, adding: mode === 'preview' ? null : state().adding });
    },

    selectPage(page: string) {
      set((s) => ({ page, selected: null, adding: s.adding && page !== s.page ? null : s.adding }));
    },

    select(tile: string | null) {
      set({ selected: tile, adding: null });
    },

    startBlank() {
      start(newLayout(state().orientation), { kind: 'ok', text: 'Started a Blank Layout.', event: 'new' });
    },

    startStarter() {
      start(starterLayout(state().orientation), { kind: 'ok', text: 'Started the Starter Layout.', event: 'starter' });
    },

    importText(text: string, file = 'file'): boolean {
      const parsed = parseLayout(text);
      if (!parsed.ok) {
        const [first] = parsed.errors;
        say('error', friendlyFileError(file, first.key), { detail: `${first.key}: ${first.message}`, event: 'open-failed' });
        return false;
      }
      const layout = parsed.layout;
      start(layout, { kind: 'ok', text: `Imported “${layout.name}”: ${describeLayout(layout)}.`, event: 'opened', ...counts(layout) });
      return true;
    },

    exportText(): string | null {
      const draft = current();
      return draft ? `${JSON.stringify(draft.layout, null, 2)}\n` : null;
    },

    say,

    openAdd(cell?: { row: number; col: number }) {
      if (!current()) return;
      set({ adding: cell ?? target(), selected: null, mode: 'edit' });
    },

    closeAdd() {
      set({ adding: null });
    },

    place(kind: AddKind) {
      const draft = current();
      if (!draft) return;
      const action = defaultAction(kind, draft.layout.pages.map((p) => p.id));
      const placed = placeAtTarget(action ?? { type: 'none' }, 'Tile');
      if (placed && action === null) editAction(placed.id, freshAction(kind, placed.pages, placed.page));
    },

    /** Places a Preset tile where Add Tile points, as a graph of `graph` when given. */
    placePreset(preset: PresetTile, graph: GraphBinding | null = null) {
      const action = parseAction(preset.action);
      if (action) placeAtTarget(action, presetLabel(preset.name), graph);
      else say('error', friendlyRefusal('action.type'), { detail: JSON.stringify(preset.action), event: 'refused' });
    },

    async addPreset(preset: PresetTile, graph: GraphBinding | null = null): Promise<Status | null> {
      if (!state().device) return null;
      if (state().busy) {
        say('error', `${name()} is busy. Wait a moment, then retry.`, { event: 'refused' });
        return state().status;
      }
      if (!current()) await api.load();
      const draft = current();
      if (!draft) return state().status;
      const action = parseAction(preset.action);
      if (!action) {
        say('error', friendlyRefusal('action.type'), { detail: JSON.stringify(preset.action), event: 'refused' });
        return state().status;
      }
      const shown = gridOf(draft.layout, state().page);
      const grid = shown ?? draft.layout.pages.find((p): p is GridPage => p.kind === 'grid') ?? null;
      const cell = grid ? freeCell(draft.layout, grid.id) : null;
      if (!grid || !cell) {
        say('error', friendlyRefusal('no free cell on this page', draft.layout, name()), { detail: 'no free cell on this page', event: 'refused' });
        return state().status;
      }
      const id = freshId(draft.layout, 'tile');
      const plain: Tile = { id, label: presetLabel(preset.name), row: cell[0], col: cell[1], rowSpan: 1, colSpan: 1, icon: null, action };
      const tile = graph ? graphTile(plain, graph) : plain;
      if (apply((l) => addTile(graph ? { ...l, schema: 2 } : l, grid.id, tile), { page: grid.id, selected: id, adding: null }) === null) {
        say('ok', `Added to ${name()}'s Layout`, { event: 'preset-added' });
      }
      return state().status;
    },

    editLabel,
    editAction,

    /** Draws the Tile as a graph of the bound stream, or with null as its label again. */
    setGraph(tile: string, binding: GraphBinding | null) {
      const page = state().page;
      if (!page) return 'no page';
      const reason = apply((l) => setGraph(l, page, tile, binding));
      if (reason === null) mark(tile, { action: undefined });
      return reason;
    },

    moveTile(tile: string, row: number, col: number) {
      const page = state().page;
      return page ? apply((l) => moveTile(l, page, tile, row, col)) : 'no page';
    },

    resizeTile(tile: string, rowSpan: number, colSpan: number) {
      const page = state().page;
      return page ? apply((l) => resizeTile(l, page, tile, rowSpan, colSpan)) : 'no page';
    },

    duplicate(tile: string) {
      const draft = current();
      const page = state().page;
      const grid = gridOf(draft?.layout, page);
      const source = grid?.tiles.find((t) => t.id === tile);
      if (!draft || !grid || !source) return;
      const fits = (row: number, col: number) => {
        if (row + source.rowSpan > grid.rows || col + source.colSpan > grid.cols) return false;
        for (let r = row; r < row + source.rowSpan; r++) for (let c = col; c < col + source.colSpan; c++) if (grid.tiles.some((t) => covers(t, r, c))) return false;
        return true;
      };
      const cells: [number, number][] = [];
      for (let row = 0; row < grid.rows; row++) for (let col = 0; col < grid.cols; col++) cells.push([row, col]);
      const after = cells.findIndex(([r, c]) => r * grid.cols + c > source.row * grid.cols + source.col);
      const order = after < 0 ? cells : [...cells.slice(after), ...cells.slice(0, after)];
      const spot = order.find(([r, c]) => fits(r, c));
      if (!spot) {
        say('error', friendlyRefusal('no free cell on this page', draft.layout, name()), { detail: 'no free cell on this page', event: 'refused' });
        return;
      }
      const id = freshId(draft.layout, 'tile');
      const copy: Tile = { ...structuredClone(source), id, row: spot[0], col: spot[1] };
      const problems = tileProblems(state(), tile);
      const placed = apply((l) => addTile(l, grid.id, copy), { selected: id });
      if (placed === null && problems) mark(id, structuredClone(problems));
    },

    clearTile(tile: string) {
      const page = state().page;
      if (page) apply((l) => removeTile(l, page, tile), { selected: null });
    },

    addPage() {
      const draft = current();
      if (!draft) return;
      const id = freshId(draft.layout, 'page');
      apply((l) => addPage(l, newGridPage(id, `Page ${l.pages.length + 1}`)), { page: id, selected: null, adding: null });
    },

    removePage(page: string) {
      apply((l) => removePage(l, page), { selected: null, adding: null });
    },

    renamePage(page: string, title: string) {
      return apply((l) => setPageTitle(l, page, title));
    },

    setGrid(rows: number, cols: number) {
      const page = state().page;
      return page ? apply((l) => setGridSize(l, page, rows, cols)) : 'no page';
    },

    renameLayout(value: string) {
      return apply((l) => setLayoutName(l, value));
    },

    setFullScreen(on: boolean) {
      return apply((l) => setFullScreen(l, on));
    },

    setIcon(tile: string, icon: string | null) {
      const page = state().page;
      return page ? apply((l) => setIcon(l, page, tile, icon)) : 'no page';
    },

    setAsset(tile: string, asset: string) {
      const page = state().page;
      return page ? apply((l) => setAsset(l, page, tile, asset)) : 'no page';
    },

    async push(): Promise<void> {
      const draft = current();
      if (!draft || !state().device || state().busy) return;
      const layout = draft.layout;
      const problem = firstProblem(state());
      if (problem) {
        set({ page: problem.page, selected: problem.tile, adding: null, mode: 'edit' });
        say('error', friendlyRefusal(problem.reason, layout, name()), { detail: problem.reason, event: 'refused' });
        return;
      }
      set({ busy: 'push', status: { kind: 'info', text: 'Sending…', event: 'pushing' } });
      const started = performance.now();
      try {
        try {
          await send(layout, draft.revision);
        } catch (first) {
          const error = toApiError(first);
          if (error.code !== 'layout.changed') throw error;
          let read: Awaited<ReturnType<typeof readLayout>>;
          try {
            read = await readLayout();
          } catch (second) {
            if (toApiError(second).code !== 'layout.not_found') throw error;
            await send(layout, null);
            return;
          }
          if ('problem' in read) throw error;
          const { orientation } = state();
          set({ conflict: { reason: 'changed', orientation, ...read } });
          say('info', `${name()} has “${read.layout.name}”: ${describeLayout(read.layout)}.`, { event: 'changed', detail: error.message, ...counts(read.layout) });
        }
      } catch (error) {
        sendFailed(toApiError(error), layout);
      } finally {
        performance.measure?.('push', { start: started });
        set({ busy: null });
      }
    },

    async load(): Promise<void> {
      if (!state().device || state().busy) return;
      set({ busy: 'load', status: { kind: 'info', text: 'Loading…', event: 'pulling' } });
      try {
        const read = await readLayout();
        if ('problem' in read) {
          say('error', `${name()}'s Layout has a problem Open TouchDeck can't read. Nothing changed.`, { detail: read.problem, event: 'pull-failed' });
          return;
        }
        const draft = current();
        if (!draft || canonical(draft.layout) === canonical(read.layout)) {
          start(read.layout, { kind: 'ok', text: `Loaded “${read.layout.name}” from ${name()}`, event: 'pulled', ...counts(read.layout) }, { onDevice: canonical(read.layout), revision: read.revision });
          return;
        }
        set({ conflict: { reason: 'load', orientation: state().orientation, ...read } });
        say('info', `${name()} has “${read.layout.name}”: ${describeLayout(read.layout)}.`, { event: 'pull-ready', ...counts(read.layout) });
      } catch (thrown) {
        const error = toApiError(thrown);
        const device = name();
        const text =
          error.code === 'layout.not_found'
            ? `${device} has no ${state().orientation} Layout yet.`
            : error.code === 'layout.stored_invalid'
              ? `${device}'s Layout has a problem Open TouchDeck can't read. Nothing changed.`
              : error.code === 'device.busy'
                ? `${device} is busy. Wait a moment, then retry.`
                : `Couldn't load from ${device}. ${PLUG_IN}`;
        say('error', text, { detail: error.message, event: 'pull-failed' });
      } finally {
        set({ busy: null });
      }
    },

    replace() {
      const conflict = state().conflict;
      if (!conflict) return;
      const { layout, revision } = conflict;
      set({ orientation: conflict.orientation });
      start(layout, { kind: 'ok', text: `Loaded “${layout.name}” from ${name()}`, event: 'pulled', ...counts(layout) }, { onDevice: canonical(layout), revision });
    },

    keepMine() {
      const conflict = state().conflict;
      if (!conflict) return;
      const draft = state().drafts[conflict.orientation];
      if (draft) keep(conflict.orientation, { ...draft, onDevice: canonical(conflict.layout), revision: conflict.revision });
      set({ conflict: null });
      say('info', 'Kept your Layout', { event: 'pull-cancelled' });
    },

    async pruneAssets(): Promise<void> {
      const { device, drafts, busy } = state();
      if (!device || busy) return;
      const keepIds = [...new Set([drafts.landscape, drafts.portrait].flatMap((d) => (d ? assetIds(d.layout) : [])))];
      set({ busy: 'prune', status: { kind: 'info', text: 'Removing unused Assets…', event: 'assets-removing' } });
      try {
        const job = await deps.companion.assets.prune({ device: device.id }, null, { also_keep: keepIds });
        const result = await deps.wait<'assets.prune'>(job);
        say('ok', `Removed ${plural(result.removed.length, 'unused Asset')} from ${name()}`, { detail: result.removed.join(', ') || 'none', event: 'assets-removed' });
      } catch (error) {
        say('error', `Couldn't remove unused Assets. Check that ${name()} is plugged in, then retry.`, { detail: toApiError(error).message, event: 'assets-failed' });
      } finally {
        set({ busy: null });
      }
    },
  });
  return api;
}

export type EditorStore = ReturnType<typeof createEditorStore>;

export function saveState(state: EditorState): SaveState {
  if (state.busy === 'push') return 'sending';
  const draft = state.drafts[state.orientation];
  if (!draft) return 'none';
  return isSent(draft) ? 'saved' : 'unsent';
}
