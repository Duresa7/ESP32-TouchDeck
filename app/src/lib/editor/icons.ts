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

function toDataUrl(rgba: Uint8ClampedArray): string {
  const canvas = document.createElement('canvas');
  canvas.width = SIDE;
  canvas.height = SIDE;
  canvas.getContext('2d')!.putImageData(new ImageData(new Uint8ClampedArray(rgba), SIDE, SIDE), 0, 0);
  return canvas.toDataURL('image/png');
}

/** Image URLs of the icon files the window has loaded, by id. */
export const iconUrls = writable<Record<string, string>>({});
const requested = new Set<string>();

/** Loads an icon file from the app once and adds its image to `iconUrls`. */
export function loadIcon(id: string) {
  if (requested.has(id)) return;
  requested.add(id);
  invoke<ArrayBuffer>('icon_file', { id })
    .then((buffer) => {
      const rgba = decodeIcon(new Uint8Array(buffer));
      if (rgba) iconUrls.update((urls) => ({ ...urls, [id]: toDataUrl(rgba) }));
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
