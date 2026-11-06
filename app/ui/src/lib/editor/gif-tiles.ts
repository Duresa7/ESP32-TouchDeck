import { createStore } from 'zustand/vanilla';
import type { AssetRef, Fit, GifTarget, Job } from '../companion/api.ts';
import type { Companion } from '../companion/client.ts';
import type { DevicesStore } from '../companion/devices.ts';
import { ApiError, toApiError } from '../companion/errors.ts';
import type { JobsStore } from '../companion/jobs.ts';
import { gifProblem, gifTarget, offersGif, REFIT_NEEDS_FILE } from './gif.ts';
import type { EditorState, EditorStore } from './store.ts';
import { assetOf, type Orientation, type Tile } from './validate.ts';

export type GifWork =
  | { state: 'queued' | 'preparing'; file: string }
  | { state: 'failed'; file: string; text: string; detail: string; retry: boolean };

export interface GifTilesState {
  /** What each Tile's GIF is doing, by Device, orientation and Tile. A Tile with nothing under way has no entry. */
  work: Record<string, GifWork>;
  /** The fit each Tile's GIF is being prepared with, by the same key, while it prepares or after it failed. */
  wanted: Record<string, Fit>;
  /** The fit and file name each GIF Asset prepared this session was made from, by Asset id. */
  prepared: Record<string, { fit: Fit; file: string }>;
}

export interface GifTilesDeps {
  editor: EditorStore;
  companion: Pick<Companion, 'assets' | 'jobs'>;
  jobs: JobsStore;
  devices: DevicesStore;
  wait(job: Job, signal: AbortSignal): Promise<AssetRef>;
}

interface Place {
  device: string;
  orientation: Orientation;
  tile: string;
}

interface Aim {
  key: string;
  target: GifTarget;
}

interface Run {
  place: Place;
  file: File;
  fit: Fit;
  aim: Aim;
  controller: AbortController;
}

interface Source {
  file: File;
  fit: Fit;
  aim: string;
}

const ORIENTATIONS: readonly Orientation[] = ['landscape', 'portrait'];
const UPLOAD_MAX = 16 * 1024 * 1024;

const keyOf = (place: Place) => `${place.device}/${place.orientation}/${place.tile}`;

const finished = (job: Job) => job.state === 'succeeded' || job.state === 'failed' || job.state === 'cancelled';

type Open = Pick<EditorState, 'device' | 'orientation'>;

function placeIn(editor: Open, tile: string): Place | null {
  return editor.device ? { device: editor.device.id, orientation: editor.orientation, tile } : null;
}

/** What the open Device's Tile is doing with its GIF, or undefined when nothing is under way. */
export function gifWork(state: GifTilesState, editor: Open, tile: string): GifWork | undefined {
  const place = placeIn(editor, tile);
  return place ? state.work[keyOf(place)] : undefined;
}

/** The fit a Tile's GIF has, or is being prepared with: `contain` until someone picks `cover`. */
export function gifFit(state: GifTilesState, editor: Open, tile: Pick<Tile, 'id' | 'asset'>): Fit {
  const place = placeIn(editor, tile.id);
  const asset = assetOf(tile);
  return (place && state.wanted[keyOf(place)]) || (asset && state.prepared[asset]?.fit) || 'contain';
}

/** Prepares GIFs for Tiles: one upload and `assets.render` Job at a time, refitted when the Tile, the Layout or the Device changes. */
export function createGifTiles(deps: GifTilesDeps) {
  const store = createStore<GifTilesState>()(() => ({ work: {}, wanted: {}, prepared: {} }));
  const runs = new Map<string, Run>();
  const sources = new Map<string, Source>();
  const retries = new Map<string, { file: File; fit: Fit }>();
  const waiting: (() => void)[] = [];
  let busy = false;

  function acquire(signal: AbortSignal): Promise<void> {
    if (!busy) {
      busy = true;
      return Promise.resolve();
    }
    return new Promise((resolve, reject) => {
      const go = () => {
        signal.removeEventListener('abort', stop);
        resolve();
      };
      const stop = () => {
        const at = waiting.indexOf(go);
        if (at >= 0) waiting.splice(at, 1);
        reject(signal.reason);
      };
      waiting.push(go);
      signal.addEventListener('abort', stop, { once: true });
    });
  }

  function release() {
    const next = waiting.shift();
    if (next) next();
    else busy = false;
  }

  function setWork(key: string, work: GifWork | null, fit?: Fit) {
    store.setState((s) => {
      const { [key]: _work, ...others } = s.work;
      const { [key]: _fit, ...rest } = s.wanted;
      return { work: work ? { ...others, [key]: work } : others, wanted: work && fit ? { ...rest, [key]: fit } : rest };
    });
  }

  function show(run: Run, state: 'queued' | 'preparing') {
    const key = keyOf(run.place);
    const now = store.getState().work[key];
    if (runs.get(key) === run && (now?.state !== state || now.file !== run.file.name)) setWork(key, { state, file: run.file.name }, run.fit);
  }

  const deviceName = (id: string) => {
    const open = deps.editor.getState().device;
    if (open?.id === id) return open.name;
    return deps.devices.getState().devices.find((d) => d.id === id)?.name ?? 'the Device';
  };

  function aimFor(place: Place, fit: Fit): Aim | null {
    const editor = deps.editor.getState();
    const caps = deps.devices.getState().devices.find((d) => d.id === place.device)?.capabilities;
    const layout = editor.drafts[place.orientation]?.layout;
    if (editor.device?.id !== place.device || !layout || !offersGif(caps)) return null;
    const target = gifTarget(layout, place.tile);
    if (!target) return null;
    return { key: JSON.stringify([place.device, caps.anim, caps.gif_frames, caps.gif_pixels, caps.asset_bytes, target, fit]), target };
  }

  function stop(key: string) {
    const run = runs.get(key);
    if (!run) return;
    runs.delete(key);
    run.controller.abort();
  }

  const noGif = () => new ApiError('asset.unsupported', 'The Device has no GIF capabilities.', { field: 'anim', limit: 'enabled', actual: 'disabled' });

  const offered = (device: string) => offersGif(deps.devices.getState().devices.find((d) => d.id === device)?.capabilities);

  function fail(place: Place, file: File, fit: Fit, error: unknown) {
    const key = keyOf(place);
    retries.set(key, { file, fit });
    setWork(key, { state: 'failed', file: file.name, ...gifProblem(error, deviceName(place.device)), retry: true }, fit);
  }

  function landed(run: Run, asset: AssetRef) {
    const key = keyOf(run.place);
    runs.delete(key);
    sources.set(asset.id, { file: run.file, fit: run.fit, aim: run.aim.key });
    setWork(key, null);
    store.setState((s) => ({ prepared: { ...s.prepared, [asset.id]: { fit: run.fit, file: run.file.name } } }));
    if (deps.editor.getState().device?.id === run.place.device) deps.editor.setGif(run.place.tile, asset.id, run.place.orientation);
  }

  async function prepare(run: Run) {
    const { signal } = run.controller;
    const key = keyOf(run.place);
    let held = false;
    let job: Job | null = null;
    let unwatch = () => {};
    try {
      await acquire(signal);
      held = true;
      show(run, 'preparing');
      const uploaded = await deps.companion.assets.upload({}, null, run.file, undefined, undefined, signal);
      const source = { kind: 'gif' as const, upload: uploaded.id, device: run.place.device, target: run.aim.target };
      job = await deps.companion.assets.create({}, null, { source, fit: run.fit }, undefined, undefined, signal);
      const watched = job;
      show(run, watched.state === 'queued' ? 'queued' : 'preparing');
      unwatch = deps.jobs.subscribe((s) => {
        const latest = s.jobs[watched.id];
        if (latest && !finished(latest)) show(run, latest.state === 'queued' ? 'queued' : 'preparing');
      });
      const asset = await deps.wait(job, signal);
      if (runs.get(key) === run) landed(run, asset);
    } catch (error) {
      if (signal.aborted) {
        if (job) deps.companion.jobs.cancel({ job: job.id }).catch(() => {});
        return;
      }
      if (runs.get(key) !== run) return;
      runs.delete(key);
      if (toApiError(error).code === 'job.cancelled') setWork(key, null);
      else fail(run.place, run.file, run.fit, error);
    } finally {
      unwatch();
      if (held) release();
    }
  }

  function start(place: Place, file: File, fit: Fit) {
    const key = keyOf(place);
    stop(key);
    retries.delete(key);
    if (!offered(place.device)) {
      fail(place, file, fit, noGif());
      return;
    }
    if (file.size > UPLOAD_MAX) {
      fail(place, file, fit, new ApiError('request.too_large', 'The GIF is larger than 16 MiB.', { limit: UPLOAD_MAX, actual: file.size, unit: 'bytes' }));
      return;
    }
    const aim = aimFor(place, fit);
    if (!aim) {
      setWork(key, null);
      return;
    }
    const run: Run = { place, file, fit, aim, controller: new AbortController() };
    runs.set(key, run);
    setWork(key, { state: busy ? 'queued' : 'preparing', file: file.name }, fit);
    void prepare(run);
  }

  function tilesOf(editor: EditorState, orientation: Orientation): Tile[] {
    return (editor.drafts[orientation]?.layout.pages ?? []).flatMap((page) => (page.kind === 'grid' ? page.tiles : []));
  }

  function refit(device: string, editor: EditorState): Set<string> {
    const present = new Set<string>();
    for (const orientation of ORIENTATIONS) {
      for (const tile of tilesOf(editor, orientation)) {
        const place = { device, orientation, tile: tile.id };
        const key = keyOf(place);
        present.add(key);
        const asset = tile.render === 'gif' ? assetOf(tile) : null;
        const source = asset ? sources.get(asset) : undefined;
        if (!source || runs.has(key) || store.getState().work[key]) continue;
        const aim = aimFor(place, source.fit);
        if (aim && aim.key !== source.aim) start(place, source.file, source.fit);
      }
    }
    return present;
  }

  function sync() {
    const editor = deps.editor.getState();
    const device = editor.device?.id ?? null;
    for (const run of [...runs.values()]) {
      const key = keyOf(run.place);
      if (runs.get(key) !== run) continue;
      const aim = run.place.device === device ? aimFor(run.place, run.fit) : null;
      if (!aim) {
        stop(key);
        if (run.place.device === device && !offered(device)) fail(run.place, run.file, run.fit, noGif());
        else setWork(key, null);
      } else if (aim.key !== run.aim.key) {
        start(run.place, run.file, run.fit);
      }
    }
    const present = device ? refit(device, editor) : new Set<string>();
    const gone = Object.keys(store.getState().work).filter((key) => !present.has(key));
    for (const key of gone) {
      retries.delete(key);
      setWork(key, null);
    }
  }

  const unsubscribe = [deps.editor.subscribe(sync), deps.devices.subscribe(sync)];

  const here = (tile: string) => placeIn(deps.editor.getState(), tile);

  const fitOf = (tile: Pick<Tile, 'id' | 'asset'>): Fit => gifFit(store.getState(), deps.editor.getState(), tile);

  function choose(tile: string, file: File) {
    const place = here(tile);
    if (!place) return;
    const shown = tilesOf(deps.editor.getState(), place.orientation).find((t) => t.id === tile);
    start(place, file, shown ? fitOf(shown) : 'contain');
  }

  return Object.assign(store, {
    /** Places a Tile where Add Tile points and prepares this GIF for it. */
    add(file: File): string | null {
      const id = deps.editor.placeGif();
      if (id) choose(id, file);
      return id;
    },

    /** Prepares this GIF for the Tile. The Tile keeps what it shows until the GIF is ready. */
    choose,

    /** Stops the Tile's GIF from preparing. The Tile keeps what it had. */
    cancel(tile: string) {
      const place = here(tile);
      if (!place) return;
      const key = keyOf(place);
      stop(key);
      retries.delete(key);
      setWork(key, null);
    },

    /** Prepares the GIF that failed again, from the same file. */
    retry(tile: string) {
      const place = here(tile);
      const again = place && retries.get(keyOf(place));
      if (place && again) start(place, again.file, again.fit);
    },

    /** Refits the Tile's GIF to the whole Tile (`cover`) or to show all of it (`contain`). A GIF from before this session needs its file again. */
    setFit(tile: string, fit: Fit) {
      const place = here(tile);
      if (!place) return;
      const key = keyOf(place);
      const run = runs.get(key);
      if (run) {
        if (run.fit !== fit) start(place, run.file, fit);
        return;
      }
      const shown = tilesOf(deps.editor.getState(), place.orientation).find((t) => t.id === tile);
      const asset = shown?.render === 'gif' ? assetOf(shown) : null;
      const source = asset ? sources.get(asset) : undefined;
      const again = retries.get(key);
      if (source) {
        if (source.fit !== fit) start(place, source.file, fit);
      } else if (again) {
        start(place, again.file, fit);
      } else {
        setWork(key, { state: 'failed', file: '', text: REFIT_NEEDS_FILE, detail: '', retry: false });
      }
    },

    fitOf,

    close() {
      for (const off of unsubscribe) off();
      for (const key of [...runs.keys()]) stop(key);
    },
  });
}

export type GifTiles = ReturnType<typeof createGifTiles>;
