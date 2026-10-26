import { invoke } from '@tauri-apps/api/core';
import { writable } from 'svelte/store';

export interface BuiltinIcon {
  name: string;
  label: string;
  tags: string[];
  id: string;
}

const SIDE = 64;
const HEADER = 12;
const COLOR = SIDE * SIDE * 2;
const FILE = HEADER + COLOR + SIDE * SIDE;

/** Decodes a `TDI1` file into RGBA, widening RGB565 the way the deck's panel shows it. */
export function decodeIcon(file: Uint8Array): Uint8ClampedArray | null {
  if (file.length !== FILE || String.fromCharCode(...file.subarray(0, 4)) !== 'TDI1') return null;
  const rgba = new Uint8ClampedArray(SIDE * SIDE * 4);
  for (let i = 0; i < SIDE * SIDE; i++) {
    const value = file[HEADER + i * 2] | (file[HEADER + i * 2 + 1] << 8);
    const r = value >> 11;
    const g = (value >> 5) & 0x3f;
    const b = value & 0x1f;
    rgba[i * 4] = (r << 3) | (r >> 2);
    rgba[i * 4 + 1] = (g << 2) | (g >> 4);
    rgba[i * 4 + 2] = (b << 3) | (b >> 2);
    rgba[i * 4 + 3] = file[HEADER + COLOR + i];
  }
  return rgba;
}

/** The `file:` id a tile names, or null for no icon or an `lv:` one. */
export function fileId(icon: string | null | undefined): string | null {
  return icon?.startsWith('file:') ? icon.slice(5) : null;
}

/** Built-in icons whose name, label, or tags contain the query. */
export function searchIcons(icons: BuiltinIcon[], query: string): BuiltinIcon[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return icons.filter((icon) => {
    const text = [icon.name, icon.label, ...icon.tags].join(' ').toLowerCase();
    return words.every((word) => text.includes(word));
  });
}

/**
 * Paints decoded icon pixels into a 64 by 64 canvas. A canvas needs no image URL, so the
 * app's content security policy stays as it is.
 */
export function paint(canvas: HTMLCanvasElement, rgba: Uint8ClampedArray | undefined) {
  const draw = (pixels: Uint8ClampedArray | undefined) => {
    canvas.width = SIDE;
    canvas.height = SIDE;
    const context = canvas.getContext('2d');
    if (!context) return;
    context.clearRect(0, 0, SIDE, SIDE);
    if (pixels) context.putImageData(new ImageData(new Uint8ClampedArray(pixels), SIDE, SIDE), 0, 0);
  };
  draw(rgba);
  return { update: draw };
}

/** Decoded pixels of the icon files the window has loaded, by id. */
export const iconPixels = writable<Record<string, Uint8ClampedArray>>({});
const requested = new Set<string>();

/** Loads an icon file from the app once and adds its pixels to `iconPixels`. */
export function loadIcon(id: string) {
  if (requested.has(id)) return;
  requested.add(id);
  invoke<ArrayBuffer>('icon_file', { id })
    .then((buffer) => {
      const rgba = decodeIcon(new Uint8Array(buffer));
      if (rgba) iconPixels.update((icons) => ({ ...icons, [id]: rgba }));
    })
    .catch(() => requested.delete(id));
}

let builtin: Promise<BuiltinIcon[]> | null = null;

export function builtinIcons(): Promise<BuiltinIcon[]> {
  builtin ??= invoke<BuiltinIcon[]>('builtin_icons').catch((error) => {
    builtin = null;
    throw error;
  });
  return builtin;
}
