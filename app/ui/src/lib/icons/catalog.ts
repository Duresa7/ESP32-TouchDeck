import type { AssetSource, CatalogHit } from '../../companion/api.ts';
import { isAbort } from '../../companion/client.ts';
import { toApiError, type ApiError } from '../../companion/errors.ts';

export const SEARCH_DELAY_MS = 200;
export const HITS_SHOWN = 48;

export type SearchState =
  | { kind: 'idle' }
  | { kind: 'loading'; query: string; hits: readonly CatalogHit[] }
  | { kind: 'done'; query: string; hits: readonly CatalogHit[] }
  | { kind: 'failed'; query: string; error: ApiError };

export interface CatalogSearchOptions {
  search(query: string, signal: AbortSignal): Promise<readonly CatalogHit[]>;
  onChange(state: SearchState): void;
  delayMs?: number;
}

export interface CatalogSearch {
  type(query: string): void;
  retry(): void;
  close(): void;
}

export function createCatalogSearch({ search, onChange, delayMs = SEARCH_DELAY_MS }: CatalogSearchOptions): CatalogSearch {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let running: AbortController | null = null;
  let query = '';
  let shown: readonly CatalogHit[] = [];
  let closed = false;

  const stop = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    running?.abort();
    running = null;
  };

  const run = () => {
    timer = null;
    const asked = query;
    const controller = new AbortController();
    running = controller;
    search(asked, controller.signal).then(
      (hits) => {
        if (running !== controller) return;
        running = null;
        shown = hits.slice(0, HITS_SHOWN);
        onChange({ kind: 'done', query: asked, hits: shown });
      },
      (error: unknown) => {
        if (running !== controller || isAbort(error)) return;
        running = null;
        onChange({ kind: 'failed', query: asked, error: toApiError(error) });
      },
    );
  };

  return {
    type(next) {
      if (closed) return;
      const trimmed = next.trim();
      if (trimmed === query) return;
      stop();
      query = trimmed;
      if (!query) {
        shown = [];
        onChange({ kind: 'idle' });
        return;
      }
      onChange({ kind: 'loading', query, hits: shown });
      timer = setTimeout(run, delayMs);
    },
    retry() {
      if (closed || !query) return;
      stop();
      onChange({ kind: 'loading', query, hits: shown });
      run();
    },
    close() {
      closed = true;
      stop();
    },
  };
}

const SCREEN_TILE: readonly [number, number, number] = [0x18, 0x20, 0x30];
const SCREEN_INK: readonly [number, number, number] = [0xf0, 0xf0, 0xf8];

function luminance([r, g, b]: readonly [number, number, number]): number {
  const linear = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

function contrast(a: readonly [number, number, number], b: readonly [number, number, number]): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

function rgbOf(hex: string | null): [number, number, number] | null {
  if (hex === null || !/^[0-9a-f]{6}$/i.test(hex)) return null;
  const value = parseInt(hex, 16);
  return [(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff];
}

export function catalogTint(hit: Pick<CatalogHit, 'source' | 'hex'>): [number, number, number] | null {
  if (hit.source !== 'simple-icons') return null;
  const brand = rgbOf(hit.hex);
  return brand && contrast(brand, SCREEN_TILE) >= 3 ? brand : [...SCREEN_INK];
}

export function catalogSource(hit: Pick<CatalogHit, 'source' | 'id' | 'hex'>): Extract<AssetSource, { kind: 'catalog' }> {
  return { kind: 'catalog', catalog: hit.source, id: hit.id, variant: 'base', padding: 0, background: null, tint: catalogTint(hit) };
}

export const SOURCE_NAMES: Readonly<Record<string, string>> = { 'simple-icons': 'Simple Icons', selfhst: 'selfh.st' };

export function monogram(name: string): string {
  const segments = [...new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(name)].map((s) => s.segment);
  return (segments.find((s) => /[\p{L}\p{N}]/u.test(s)) ?? segments[0] ?? '?').toLocaleUpperCase();
}
