import type { AnimationCapabilities, Capabilities, GifTarget } from '../companion/api.ts';
import { toApiError } from '../companion/errors.ts';
import { geometry, screenSize, tileRect } from '../deck/geometry.ts';
import { THEME } from '../deck/theme.ts';
import { UNREACHABLE } from '../icons/pick.ts';
import type { Layout } from './validate.ts';

export type GifCaps = Capabilities & { anim: AnimationCapabilities };

/** Whether the Device takes animated GIF Tiles: the same test the Companion applies before it prepares one. */
export function offersGif(caps: Capabilities | null | undefined): caps is GifCaps {
  const anim = caps?.anim;
  return (
    !!caps &&
    !!anim &&
    anim.image.every((side) => side > 0) &&
    anim.min_delay_ms > 0 &&
    anim.page_tiles > 0 &&
    anim.page_pixels > 0 &&
    caps.formats.includes('gif') &&
    caps.tile_kinds.includes('gif') &&
    (caps.gif_frames ?? 0) > 0 &&
    (caps.gif_pixels ?? 0) > 0
  );
}

const hexRgb = (hex: string): [number, number, number] => {
  const n = Number.parseInt(hex.slice(1), 16);
  return [n >> 16, (n >> 8) & 0xff, n & 0xff];
};

/** The Tile region on the Device, in Device pixels, that a GIF for this Tile is fitted to. */
export function gifTarget(layout: Layout, tile: string): GifTarget | null {
  const g = geometry(screenSize(layout.orientation), layout.fullScreen === true);
  for (const page of layout.pages) {
    if (page.kind !== 'grid') continue;
    const found = page.tiles.find((t) => t.id === tile);
    if (!found) continue;
    const rect = tileRect(page, found, g.grid, g.gap);
    return rect.w > 0 && rect.h > 0 ? { width: rect.w, height: rect.h, background: hexRgb(THEME.color.tile) } : null;
  }
  return null;
}

export interface GifInfo {
  width: number;
  height: number;
  frames: number;
  durationMs: number;
}

const TRAILER = 0x3b;
const EXTENSION = 0x21;
const IMAGE = 0x2c;
const CONTROL = 0xf9;

const u16 = (bytes: Uint8Array, at: number) => bytes[at] | (bytes[at + 1] << 8);
const tableBytes = (packed: number) => (packed & 0x80 ? 3 * (1 << ((packed & 7) + 1)) : 0);

function skipBlocks(bytes: Uint8Array, at: number): number | null {
  let i = at;
  while (i < bytes.length) {
    const size = bytes[i];
    if (size === 0) return i + 1;
    i += size + 1;
  }
  return null;
}

/** The canvas, frame count and loop length of a GIF, and its first frame alone as a GIF that does not loop. */
export function readGif(bytes: Uint8Array): { info: GifInfo; poster: Uint8Array<ArrayBuffer> } | null {
  const signature = String.fromCharCode(...bytes.subarray(0, 6));
  if (bytes.length < 13 || (signature !== 'GIF89a' && signature !== 'GIF87a')) return null;
  const head = 13 + tableBytes(bytes[10]);
  if (head > bytes.length) return null;
  let i = head;
  let frames = 0;
  let durationMs = 0;
  let delayCs = 0;
  let control: [number, number] | null = null;
  let first: [number, number] | null = null;
  let firstControl: [number, number] | null = null;
  while (i < bytes.length) {
    const block = bytes[i];
    if (block === TRAILER) {
      if (!first || frames === 0) return null;
      const parts = [bytes.subarray(0, head), firstControl ? bytes.subarray(...firstControl) : new Uint8Array(0), bytes.subarray(...first), new Uint8Array([TRAILER])];
      const poster = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
      let at = 0;
      for (const part of parts) {
        poster.set(part, at);
        at += part.length;
      }
      return { info: { width: u16(bytes, 6), height: u16(bytes, 8), frames, durationMs }, poster };
    }
    if (block === EXTENSION) {
      if (i + 2 > bytes.length) return null;
      const end = skipBlocks(bytes, i + 2);
      if (end === null) return null;
      if (bytes[i + 1] === CONTROL && bytes[i + 2] >= 4) {
        delayCs = u16(bytes, i + 4);
        control = [i, end];
      }
      i = end;
    } else if (block === IMAGE) {
      const data = i + 10 + tableBytes(bytes[i + 9]);
      if (data + 1 > bytes.length) return null;
      const end = skipBlocks(bytes, data + 1);
      if (end === null) return null;
      if (!first) {
        first = [i, end];
        firstControl = control;
      }
      frames += 1;
      durationMs += delayCs * 10;
      delayCs = 0;
      control = null;
      i = end;
    } else {
      return null;
    }
  }
  return null;
}

/** "Fitted to Desk: 128 × 92, 10 fps." A still GIF has no frame rate. */
export function fittedText(device: string, info: GifInfo): string {
  const fps = info.frames > 1 && info.durationMs > 0 ? Number(((info.frames * 1000) / info.durationMs).toFixed(1)) : null;
  return `Fitted to ${device}: ${info.width} × ${info.height}${fps === null ? '' : `, ${fps} fps`}.`;
}

export interface GifProblem {
  text: string;
  detail: string;
}

const tooLong = (device: string) => `This GIF is too long for ${device}. Choose a shorter GIF.`;
const tooLarge = (device: string) => `This GIF is too large for ${device}. Choose a smaller GIF.`;
const DECODED = 'This GIF needs more than 64 MiB to decode all its frames. Choose a smaller or shorter GIF.';
const PREPARE_FAILED = "Couldn't prepare this GIF. Choose another GIF.";
const STORE_FULL = "This PC's Asset store is full. Retry in an hour, or remove Tiles you don't use and send the Layout.";
const BUSY = 'Other Assets are still being prepared. Wait a moment, then retry.';
export const REFIT_NEEDS_FILE = 'Choose this GIF again to refit it.';

const field = (details: unknown, name: string): unknown => (typeof details === 'object' && details !== null && Object.hasOwn(details, name) ? Reflect.get(details, name) : undefined);

function detailOf(code: string, message: string, details: unknown): string {
  return details === null || details === undefined ? `${code}: ${message}` : `${code}: ${message} ${JSON.stringify(details)}`;
}

function gifSentence(code: string, details: unknown, device: string): string | null {
  switch (code) {
    case 'asset.too_large': {
      const name = field(details, 'field');
      if (name === 'gif_frames' || name === 'source_frames') return tooLong(device);
      if (name === 'decoded_rgba') return DECODED;
      return tooLarge(device);
    }
    case 'asset.too_short': {
      const limit = field(details, 'limit');
      return `This GIF is too short for ${device}. Choose a GIF that plays for at least ${typeof limit === 'number' ? limit : 100} ms.`;
    }
    case 'asset.render_failed':
      return PREPARE_FAILED;
    case 'asset.store_full':
      return STORE_FULL;
    case 'asset.unsupported':
      return `${device} can't play this GIF. Update its Firmware, then retry.`;
    case 'layout.animation_limit':
      return `This Page has too many animations for ${device}. Remove an animated Tile.`;
    case 'asset.refused': {
      const reason = field(details, 'reason');
      if (reason === 'full') return `${device} is out of space. Remove unused Assets, then retry.`;
      if (reason === 'memory') return tooLarge(device);
      return null;
    }
    default:
      return null;
  }
}

/** The copy guide's sentence for a GIF that couldn't be prepared, with the raw reason for Details. */
export function gifProblem(error: unknown, device: string): GifProblem {
  const { code, message, details } = toApiError(error);
  const detail = detailOf(code, message, details);
  const known = gifSentence(code, details, device);
  if (known) return { text: known, detail };
  switch (code) {
    case 'request.too_large':
      return { text: tooLarge(device), detail };
    case 'limits.exceeded':
      return { text: BUSY, detail };
    case 'client.unreachable':
      return { text: UNREACHABLE, detail };
    case 'device.not_connected':
    case 'device.not_found':
      return { text: `Couldn't prepare this GIF. Check that ${device} is plugged in, then retry.`, detail };
    default:
      return { text: PREPARE_FAILED, detail };
  }
}

/** The sentence for a Layout push the Device or Companion refused because of its GIFs, or null when the refusal is about something else. */
export function gifPushProblem(error: unknown, device: string): GifProblem | null {
  const { code, message, details } = toApiError(error);
  const text = gifSentence(code, details, device);
  return text ? { text, detail: detailOf(code, message, details) } : null;
}
