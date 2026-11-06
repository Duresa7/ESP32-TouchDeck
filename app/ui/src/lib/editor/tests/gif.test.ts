import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, test } from 'vitest';
import { GIF_CAPS } from '../../companion/samples.ts';
import { ApiError } from '../../companion/errors.ts';
import { fittedText, gifProblem, gifPushProblem, gifTarget, offersGif, readGif } from '../gifs.ts';
import { parseLayout, setGif, starterLayout } from '../../layout/model.ts';

const HEADER = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61];
const LOOP = [0x21, 0xff, 0x0b, ...[...'NETSCAPE2.0'].map((c) => c.charCodeAt(0)), 0x03, 0x01, 0x00, 0x00, 0x00];

function gifBytes({ width = 2, height = 3, delays = [10, 10] }: { width?: number; height?: number; delays?: number[] } = {}): Uint8Array {
  const out = [...HEADER, width & 0xff, width >> 8, height & 0xff, height >> 8, 0x80, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, ...LOOP];
  delays.forEach((delay, i) => {
    out.push(0x21, 0xf9, 0x04, 0x04, delay & 0xff, delay >> 8, 0x00, 0x00);
    out.push(0x2c, 0, 0, 0, 0, width & 0xff, width >> 8, height & 0xff, height >> 8, 0x00, 0x02, 0x02, 0x44, i + 1, 0x00);
  });
  out.push(0x3b);
  return new Uint8Array(out);
}

describe('which Devices get a GIF choice', () => {
  test('a Device with anim, the gif format and Tile kind, and nonzero frame and pixel limits offers GIF', () => {
    expect(offersGif(GIF_CAPS)).toBe(true);
  });

  test.each([
    ['no capabilities', null],
    ['no anim', { ...GIF_CAPS, anim: undefined }],
    ['no gif format', { ...GIF_CAPS, formats: ['tdi1'] }],
    ['no gif Tile kind', { ...GIF_CAPS, tile_kinds: ['static'] }],
    ['zero frames', { ...GIF_CAPS, gif_frames: 0 }],
    ['zero pixels', { ...GIF_CAPS, gif_pixels: 0 }],
    ['an empty canvas', { ...GIF_CAPS, anim: { ...GIF_CAPS.anim!, image: [0, 128] as [number, number] } }],
  ])('a Device with %s offers no GIF', (_, caps) => {
    expect(offersGif(caps)).toBe(false);
  });
});

describe('the canvas a GIF is fitted to', () => {
  test('is the Tile region on the Device, in Device pixels, over the Tile background', () => {
    const layout = starterLayout('landscape');
    expect(gifTarget(layout, 'prev')).toEqual({ width: 312, height: 226, background: [24, 32, 49] });
  });

  test('follows the Tile size, the grid, full screen and the orientation', () => {
    const layout = starterLayout('landscape');
    const media = layout.pages[0];
    if (media.kind !== 'grid') throw new Error('media is a grid');
    media.tiles[0] = { ...media.tiles[0], colSpan: 2 };
    media.tiles.splice(1, 1);
    expect(gifTarget(layout, 'prev')).toMatchObject({ width: 640, height: 226 });
    expect(gifTarget({ ...layout, fullScreen: true }, 'prev')).toMatchObject({ width: 677, height: 292 });
    expect(gifTarget(starterLayout('portrait'), 'prev')).toMatchObject({ width: 170, height: 438 });
  });

  test('is unknown for a Tile the Layout lacks', () => {
    expect(gifTarget(starterLayout('landscape'), 'ghost')).toBeNull();
  });
});

describe('reading a prepared GIF', () => {
  test('gives its canvas, frame count and loop length', () => {
    expect(readGif(gifBytes({ width: 120, height: 90, delays: [10, 10, 20] }))?.info).toEqual({ width: 120, height: 90, frames: 3, durationMs: 400 });
  });

  test('cuts a poster: the first frame alone, as a GIF that does not loop', () => {
    const read = readGif(gifBytes({ delays: [10, 30] }));
    expect(read).not.toBeNull();
    expect(readGif(read!.poster)?.info).toEqual({ width: 2, height: 3, frames: 1, durationMs: 100 });
    expect(Buffer.from(read!.poster).includes(Buffer.from('NETSCAPE2.0'))).toBe(false);
    expect([...read!.poster.slice(-4)]).toEqual([0x44, 0x01, 0x00, 0x3b]);
  });

  test.each([
    ['not a GIF', new TextEncoder().encode('PNG, not a GIF at all')],
    ['a header alone', new Uint8Array(HEADER)],
    ['a frame cut short', gifBytes().slice(0, -6)],
    ['no trailer', gifBytes().slice(0, -1)],
    ['an unknown block', new Uint8Array([...gifBytes().slice(0, -1), 0x99, 0x3b])],
    ['no frame', new Uint8Array([...gifBytes({ delays: [] })])],
    ['a sub-block running past the end', new Uint8Array([...gifBytes().slice(0, -1), 0x21, 0xfe, 0xff, 0x41])],
  ])('refuses %s', (_, bytes) => {
    expect(readGif(bytes)).toBeNull();
  });

  test('stays bounded by the bytes it is given', () => {
    const frames = 5000;
    const bytes = gifBytes({ delays: Array.from({ length: frames }, () => 10) });
    expect(readGif(bytes)?.info.frames).toBe(frames);
  });
});

describe('the Fitted line', () => {
  test('names the Device, the canvas and the frame rate', () => {
    expect(fittedText('Desk', { width: 128, height: 92, frames: 20, durationMs: 2000 })).toBe('Fitted to Desk: 128 × 92, 10 fps.');
    expect(fittedText('Desk', { width: 64, height: 64, frames: 3, durationMs: 900 })).toBe('Fitted to Desk: 64 × 64, 3.3 fps.');
  });

  test('leaves out the frame rate of a still GIF', () => {
    expect(fittedText('Desk', { width: 64, height: 64, frames: 1, durationMs: 100 })).toBe('Fitted to Desk: 64 × 64.');
  });
});

describe('every GIF refusal has its sentence', () => {
  const say = (code: string, details: unknown = null) => gifProblem(new ApiError(code, 'raw message', details), 'Desk');

  test.each([
    ['gif_frames', 'This GIF is too long for Desk. Choose a shorter GIF.'],
    ['source_frames', 'This GIF is too long for Desk. Choose a shorter GIF.'],
    ['decoded_rgba', 'This GIF needs more than 64 MiB to decode all its frames. Choose a smaller or shorter GIF.'],
    ['asset_bytes', 'This GIF is too large for Desk. Choose a smaller GIF.'],
    ['gif_pixels', 'This GIF is too large for Desk. Choose a smaller GIF.'],
    ['source_bytes', 'This GIF is too large for Desk. Choose a smaller GIF.'],
    ['source_width', 'This GIF is too large for Desk. Choose a smaller GIF.'],
    ['device_limit', 'This GIF is too large for Desk. Choose a smaller GIF.'],
  ])('asset.too_large for %s', (field, text) => {
    expect(say('asset.too_large', { field, limit: 1, actual: 2, unit: 'frames' }).text).toBe(text);
  });

  test('asset.too_short names the shortest loop', () => {
    expect(say('asset.too_short', { field: 'duration_ms', limit: 100, actual: 40, unit: 'ms' }).text).toBe('This GIF is too short for Desk. Choose a GIF that plays for at least 100 ms.');
  });

  test.each(['not_gif', 'corrupt', 'timed_out', 'invalid_limits', 'invalid'])('asset.render_failed for %s', (reason) => {
    expect(say('asset.render_failed', { reason }).text).toBe("Couldn't prepare this GIF. Choose another GIF.");
  });

  test('asset.store_full', () => {
    expect(say('asset.store_full', { limit: 1, actual: 2, unit: 'bytes' }).text).toBe("This PC's Asset store is full. Retry in an hour, or remove Tiles you don't use and send the Layout.");
  });

  test('asset.unsupported, from the Companion or a format refusal', () => {
    expect(say('asset.unsupported', { field: 'anim', limit: 'enabled', actual: 'disabled', reason: 'unsupported' }).text).toBe("Desk can't play this GIF. Update its Firmware, then retry.");
    expect(say('asset.unsupported', { field: 'format', limit: 'supported encoding', actual: 'refused', reason: 'format' }).text).toBe("Desk can't play this GIF. Update its Firmware, then retry.");
  });

  test('layout.animation_limit', () => {
    expect(say('layout.animation_limit', { page: 'media', field: 'page_tiles', limit: 4, actual: 5 }).text).toBe('This Page has too many animations for Desk. Remove an animated Tile.');
  });

  test.each([
    ['full', 'Desk is out of space. Remove unused Assets, then retry.'],
    ['memory', 'This GIF is too large for Desk. Choose a smaller GIF.'],
  ])('a link refusal with reason %s', (reason, text) => {
    expect(say('asset.refused', { reason }).text).toBe(text);
  });

  test('a link refusal with reason size comes as asset.too_large with no measurement', () => {
    expect(say('asset.too_large', { reason: 'size', field: 'device_limit', limit: null, actual: null, unit: 'unknown' }).text).toBe('This GIF is too large for Desk. Choose a smaller GIF.');
  });

  test.each([
    ['request.too_large', 'This GIF is too large for Desk. Choose a smaller GIF.'],
    ['request.media_type', "Couldn't prepare this GIF. Choose another GIF."],
    ['limits.exceeded', 'Other Assets are still being prepared. Wait a moment, then retry.'],
    ['client.unreachable', "Can't reach the Companion. Check that Open TouchDeck is running in the tray, then retry."],
    ['device.not_connected', "Couldn't prepare this GIF. Check that Desk is plugged in, then retry."],
    ['something.new', "Couldn't prepare this GIF. Choose another GIF."],
  ])('%s', (code, text) => {
    expect(say(code).text).toBe(text);
  });

  test('keeps the raw code, message and details for Details', () => {
    expect(say('asset.too_large', { field: 'gif_frames', limit: 60, actual: 61, unit: 'frames' }).detail).toBe('asset.too_large: raw message {"field":"gif_frames","limit":60,"actual":61,"unit":"frames"}');
    expect(say('asset.render_failed').detail).toBe('asset.render_failed: raw message');
  });

  test('says nothing of its own for a push failure that is not about a GIF', () => {
    expect(gifPushProblem(new ApiError('asset.refused', 'raw', { reason: 'busy' }), 'Desk')).toBeNull();
    expect(gifPushProblem(new ApiError('device.timeout', 'raw'), 'Desk')).toBeNull();
    expect(gifPushProblem(new ApiError('layout.animation_limit', 'raw', { page: 'media' }), 'Desk')?.text).toBe('This Page has too many animations for Desk. Remove an animated Tile.');
  });
});

describe('turning a Live tile into a GIF Tile', () => {
  test('drops what the Live tile bound, so the GIF Tile stays valid', () => {
    const parsed = parseLayout(readFileSync(join(import.meta.dirname, '..', '..', '..', '..', '..', '..', 'schema', 'examples', 'media.json'), 'utf8'));
    if (!parsed.ok) throw new Error('media.json breaks the Layout rules');
    const page = parsed.layout.pages[0].id;
    const done = setGif(parsed.layout, page, 'volume', 'b'.repeat(64));
    if (!done.ok) throw new Error(done.reason);
    const grid = done.layout.pages[0];
    expect(grid.kind === 'grid' && grid.tiles.find((t) => t.id === 'volume')).toEqual(expect.not.objectContaining({ input: expect.anything(), streams: expect.anything() }));
    expect(grid.kind === 'grid' && grid.tiles.find((t) => t.id === 'volume')).toMatchObject({ render: 'gif', icon: null, asset: `sha256:${'b'.repeat(64)}` });
  });
});
