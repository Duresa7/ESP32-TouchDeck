import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import type { AssetSource, Capabilities, DeviceSummary } from '../companion/api.ts';
import { createClient, type Companion } from '../companion/client.ts';
import { createDevicesStore, type DevicesStore } from '../companion/devices.ts';
import { createFakeCompanion, type FakeCompanion } from '../companion/fake.ts';
import { createJobsStore, waitForJob, type JobsStore } from '../companion/jobs.ts';
import { GIF_CAPS, sampleDevice } from '../companion/samples.ts';
import { windowSession } from '../companion/session.ts';
import { createGifTiles, gifWork, type GifTiles } from './gif-tiles.ts';
import { createEditorStore, type EditorStore } from './store.ts';
import { assetOf, starterLayout, type GridPage, type Tile } from './validate.ts';

const BASE = 'http://127.0.0.1:51234';
const connected = { state: 'connected' as const, port: 'COM3', since_ms: 0, protocol: 2 };

let fake: FakeCompanion;
let companion: Companion;
let jobs: JobsStore;
let devices: DevicesStore;
let editor: EditorStore;
let gifs: GifTiles;

const desk = (capabilities: Capabilities | null = GIF_CAPS): DeviceSummary => ({ ...sampleDevice('mock0001', 'Desk', connected), capabilities });
const studio = (): DeviceSummary => ({ ...sampleDevice('mock0002', 'Studio', connected), capabilities: { ...GIF_CAPS, anim: { ...GIF_CAPS.anim!, image: [96, 96] } } });

function gifFile(name = 'cat.gif', seed = 1): File {
  return new File([new Uint8Array([0x47, 0x49, 0x46, 0x38, 0x39, 0x61, seed])], name, { type: 'image/gif' });
}

const layout = () => editor.getState().drafts[editor.getState().orientation]!.layout;
const grid = (id = 'media'): GridPage => {
  const page = layout().pages.find((p) => p.id === id);
  if (page?.kind !== 'grid') throw new Error(`${id} is not a grid`);
  return page;
};
const tile = (id: string, page = 'media'): Tile => grid(page).tiles.find((t) => t.id === id)!;
const work = (id: string) => gifWork(gifs.getState(), editor.getState(), id);
const gifSources = () => fake.assets.created.filter((s): s is Extract<AssetSource, { kind: 'gif' }> => s.kind === 'gif');
const settled = (id: string) => vi.waitFor(() => expect(work(id)).toBeUndefined());

beforeEach(() => {
  fake = createFakeCompanion({ devices: [desk(), studio()], jobMs: 5, assets: { renderMs: 5 } });
  vi.stubGlobal('fetch', fake.fetch);
  companion = createClient(BASE, windowSession(fake.bridge));
  jobs = createJobsStore();
  devices = createDevicesStore();
  devices.getState().sync([desk(), studio()]);
  editor = createEditorStore({ companion, storage: null, wait: (job) => waitForJob(jobs, companion, job, { pollMs: 5 }) });
  gifs = createGifTiles({ editor, companion, jobs, devices, wait: (job, signal) => waitForJob<'assets.render'>(jobs, companion, job, { pollMs: 5, signal }) });
  editor.open({ id: 'mock0001', name: 'Desk' });
  editor.startStarter();
});

afterEach(() => {
  gifs.close();
  vi.unstubAllGlobals();
});

describe('adding a GIF Tile', () => {
  test('places a GIF Tile where Add Tile points, fitted to that Tile region', async () => {
    editor.setGrid(2, 4);
    editor.openAdd({ row: 0, col: 3 });
    gifs.add(gifFile());
    const id = editor.getState().selected!;
    expect(tile(id)).toMatchObject({ label: 'GIF', row: 0, col: 3 });
    expect(work(id)).toMatchObject({ state: 'preparing', file: 'cat.gif' });
    await settled(id);
    expect(tile(id)).toMatchObject({ render: 'gif', icon: null });
    expect(assetOf(tile(id))).toMatch(/^[0-9a-f]{64}$/);
    expect(layout().schema).toBe(2);
    expect(gifSources()).toEqual([{ kind: 'gif', upload: expect.any(String), device: 'mock0001', target: { width: 230, height: 226, background: [24, 32, 49] } }]);
  });

  test('turns an existing Tile into a GIF Tile, and its old icon goes', async () => {
    gifs.choose('prev', gifFile());
    await settled('prev');
    expect(tile('prev')).toMatchObject({ render: 'gif', icon: null, label: 'Previous' });
  });

  test('shows queued while another GIF prepares, then preparing, then the GIF', async () => {
    fake.assets.holdGifs(true);
    gifs.choose('prev', gifFile());
    await vi.waitFor(() => expect(work('prev')).toMatchObject({ state: 'queued', file: 'cat.gif' }));
    fake.assets.holdGifs(false);
    await vi.waitFor(() => expect(work('prev')?.state).not.toBe('queued'));
    await settled('prev');
    expect(tile('prev').render).toBe('gif');
  });

  test('cancels a queued GIF, and the Tile keeps what it had', async () => {
    fake.assets.holdGifs(true);
    gifs.choose('prev', gifFile());
    await vi.waitFor(() => expect(work('prev')?.state).toBe('queued'));
    gifs.cancel('prev');
    await settled('prev');
    await vi.waitFor(() => expect(fake.assets.jobs().at(-1)?.state).toBe('cancelled'));
    expect(tile('prev')).toMatchObject({ icon: 'lv:prev' });
    expect(tile('prev').render).toBeUndefined();
  });

  test('a failed preparation says why, keeps the Tile, and retries the same GIF', async () => {
    fake.assets.failGif({ code: 'asset.too_large', message: 'This GIF is too long. Choose a shorter GIF.', details: { field: 'gif_frames', limit: 60, actual: 90, unit: 'frames' } });
    gifs.choose('prev', gifFile());
    await vi.waitFor(() => expect(work('prev')).toMatchObject({ state: 'failed', text: 'This GIF is too long for Desk. Choose a shorter GIF.' }));
    expect(work('prev')).toMatchObject({ detail: expect.stringContaining('gif_frames') });
    expect(tile('prev')).toMatchObject({ icon: 'lv:prev' });
    gifs.retry('prev');
    await settled('prev');
    expect(tile('prev').render).toBe('gif');
    expect(gifSources()).toHaveLength(2);
  });

  test('a replacement that fails keeps the GIF the Tile had', async () => {
    gifs.choose('prev', gifFile('first.gif', 1));
    await settled('prev');
    const before = tile('prev').asset;
    fake.assets.failGif({ code: 'asset.render_failed', message: "Couldn't prepare this GIF. Choose another GIF.", details: { reason: 'corrupt' } });
    gifs.choose('prev', gifFile('second.gif', 2));
    await vi.waitFor(() => expect(work('prev')?.state).toBe('failed'));
    expect(tile('prev').asset).toBe(before);
  });

  test('a GIF the Tile left behind while preparing is dropped', async () => {
    fake.assets.holdGifs(true);
    gifs.choose('prev', gifFile());
    await vi.waitFor(() => expect(work('prev')?.state).toBe('queued'));
    editor.clearTile('prev');
    fake.assets.holdGifs(false);
    await vi.waitFor(() => expect(fake.assets.jobs().at(-1)?.state).toMatch(/succeeded|cancelled/));
    expect(grid().tiles.some((t) => t.id === 'prev')).toBe(false);
  });
});

describe('fitting', () => {
  test('a new GIF fits contain; Fill Tile refits it as cover from the same file', async () => {
    gifs.choose('prev', gifFile());
    await settled('prev');
    const contained = tile('prev').asset;
    expect(gifs.fitOf(tile('prev'))).toBe('contain');
    gifs.setFit('prev', 'cover');
    await settled('prev');
    expect(tile('prev').asset).not.toBe(contained);
    expect(gifs.fitOf(tile('prev'))).toBe('cover');
    expect(fake.assets.created.at(-1)).toMatchObject({ kind: 'gif', target: { width: 312, height: 226 } });
    expect(fake.assets.fits.at(-1)).toBe('cover');
  });

  test('a resize refits to the new Tile region', async () => {
    editor.setGrid(2, 4);
    gifs.choose('prev', gifFile());
    await settled('prev');
    const before = tile('prev').asset;
    editor.clearTile('playpause');
    expect(editor.resizeTile('prev', 1, 2)).toBeNull();
    await vi.waitFor(() => expect(gifSources().at(-1)?.target).toMatchObject({ width: 476, height: 226 }));
    await settled('prev');
    expect(tile('prev').asset).not.toBe(before);
  });

  test('a GIF chosen while the Tile resizes is fitted to the size it ends at', async () => {
    fake.assets.holdGifs(true);
    editor.clearTile('playpause');
    gifs.choose('prev', gifFile());
    await vi.waitFor(() => expect(work('prev')?.state).toBe('queued'));
    expect(editor.resizeTile('prev', 1, 2)).toBeNull();
    fake.assets.holdGifs(false);
    await settled('prev');
    expect(gifSources().at(-1)?.target).toMatchObject({ width: 640, height: 226 });
  });

  test('a full screen Layout refits its GIFs', async () => {
    gifs.choose('prev', gifFile());
    await settled('prev');
    editor.setFullScreen(true);
    await vi.waitFor(() => expect(gifSources().at(-1)?.target).toMatchObject({ width: 330, height: 292 }));
    await settled('prev');
  });

  test('another Device, or new limits on the same one, refit the GIF', async () => {
    gifs.choose('prev', gifFile());
    await settled('prev');
    devices.getState().upsert({ ...desk({ ...GIF_CAPS, anim: { ...GIF_CAPS.anim!, image: [96, 96] } }), rev: 2 });
    await vi.waitFor(() => expect(gifSources()).toHaveLength(2));
    await settled('prev');
    const exported = editor.exportText()!;
    editor.open({ id: 'mock0002', name: 'Studio' });
    editor.importText(exported, 'gif.json');
    await vi.waitFor(() => expect(gifSources().at(-1)).toMatchObject({ device: 'mock0002' }));
    await settled('prev');
  });

  test('a GIF chosen before this session asks for its file again to refit', () => {
    editor.importText(
      JSON.stringify({
        ...starterLayout('landscape'),
        schema: 2,
        pages: [{ id: 'media', title: 'Media', kind: 'grid', rows: 2, cols: 3, tiles: [{ id: 'prev', label: 'GIF', row: 0, col: 0, rowSpan: 1, colSpan: 1, icon: null, render: 'gif', asset: `sha256:${'a'.repeat(64)}`, action: { type: 'none' } }] }],
      }),
      'old.json',
    );
    gifs.setFit('prev', 'cover');
    expect(work('prev')).toMatchObject({ state: 'failed', text: 'Choose this GIF again to refit it.', retry: false });
    expect(gifSources()).toHaveLength(0);
  });

  test('a GIF preparing when the Device stops playing GIFs says why, and can be retried', async () => {
    fake.assets.holdGifs(true);
    gifs.choose('prev', gifFile());
    await vi.waitFor(() => expect(work('prev')?.state).toBe('queued'));
    devices.getState().upsert({ ...desk(null), rev: 4 });
    expect(work('prev')).toMatchObject({ state: 'failed', text: "Desk can't play this GIF. Update its Firmware, then retry.", retry: true });
    await vi.waitFor(() => expect(fake.assets.jobs().at(-1)?.state).toBe('cancelled'));
    fake.assets.holdGifs(false);
    devices.getState().upsert({ ...desk(), rev: 5 });
    gifs.retry('prev');
    await settled('prev');
    expect(tile('prev').render).toBe('gif');
  });

  test('a GIF over the 16 MiB upload limit is refused before it is sent', () => {
    const huge = new File([new Uint8Array(16 * 1024 * 1024 + 1)], 'huge.gif', { type: 'image/gif' });
    gifs.choose('prev', huge);
    expect(work('prev')).toMatchObject({ state: 'failed', file: 'huge.gif', text: 'This GIF is too large for Desk. Choose a smaller GIF.', detail: expect.stringContaining('request.too_large') });
    expect(gifSources()).toHaveLength(0);
  });

  test('a Device without GIF gets no preparation', () => {
    devices.getState().upsert({ ...desk(null), rev: 3 });
    gifs.choose('prev', gifFile());
    expect(work('prev')).toMatchObject({ state: 'failed', text: "Desk can't play this GIF. Update its Firmware, then retry." });
    expect(gifSources()).toHaveLength(0);
  });
});
