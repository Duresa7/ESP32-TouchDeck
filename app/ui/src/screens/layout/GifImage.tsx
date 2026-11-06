import { useEffect, useState, useSyncExternalStore, type ReactNode } from 'react';
import type { Companion } from '../../lib/companion/client.ts';
import { useCompanion } from '../../lib/companion/connection.ts';
import { isApiError } from '../../lib/companion/errors.ts';
import { readGif, type GifInfo } from '../../lib/editor/gifs.ts';

export interface GifFile {
  url: string;
  poster: string;
  info: GifInfo;
}

const KEPT = 48;
const files = new Map<string, GifFile | 'missing'>();
const loading = new Map<string, Promise<GifFile | 'missing'>>();

function remember(id: string, file: GifFile | 'missing') {
  files.set(id, file);
  const oldest = files.size > KEPT ? files.keys().next() : null;
  if (!oldest || oldest.done) return;
  const dropped = files.get(oldest.value);
  files.delete(oldest.value);
  if (dropped && dropped !== 'missing') {
    URL.revokeObjectURL(dropped.url);
    URL.revokeObjectURL(dropped.poster);
  }
}

async function fetchGif(companion: Companion, id: string): Promise<GifFile | 'missing'> {
  try {
    const blob = await companion.assets.file({ asset: id });
    const bytes = new Uint8Array(await blob.arrayBuffer());
    const read = readGif(bytes);
    if (!read) return 'missing';
    return {
      url: URL.createObjectURL(new Blob([bytes], { type: 'image/gif' })),
      poster: URL.createObjectURL(new Blob([read.poster], { type: 'image/gif' })),
      info: read.info,
    };
  } catch (error) {
    if (isApiError(error, 'asset.not_found')) return 'missing';
    throw error;
  }
}

function load(companion: Companion, id: string): Promise<GifFile | 'missing'> {
  let pending = loading.get(id);
  if (!pending) {
    pending = fetchGif(companion, id).then((file) => {
      remember(id, file);
      return file;
    });
    pending.finally(() => loading.delete(id)).catch(() => {});
    loading.set(id, pending);
  }
  return pending;
}

/** A GIF Asset's file, its poster and what it holds, or 'missing', or undefined while it loads. */
export function useGifFile(id: string | null): GifFile | 'missing' | undefined {
  const companion = useCompanion();
  const [file, setFile] = useState(() => (id ? files.get(id) : undefined));
  useEffect(() => {
    if (!id) {
      setFile(undefined);
      return;
    }
    const known = files.get(id);
    setFile(known);
    if (known !== undefined) return;
    let live = true;
    load(companion, id).then(
      (result) => live && setFile(result),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [companion, id]);
  return file;
}

const REDUCE = '(prefers-reduced-motion: reduce)';

function subscribeMotion(change: () => void) {
  if (typeof matchMedia !== 'function') return () => {};
  const query = matchMedia(REDUCE);
  query.addEventListener('change', change);
  return () => query.removeEventListener('change', change);
}

/** Whether the person asked for less motion, kept current while it changes. */
export function useReducedMotion(): boolean {
  return useSyncExternalStore(
    subscribeMotion,
    () => typeof matchMedia === 'function' && matchMedia(REDUCE).matches,
    () => false,
  );
}

/** A GIF Asset that plays, or shows its first frame alone when reduced motion is asked for. `size` fits it in a square that big; without it, it keeps its own size. `missing` stands in for a GIF this PC doesn't have. */
export function GifImage({ id, size, missing }: { id: string; size?: number; missing?: ReactNode }) {
  const file = useGifFile(id);
  const reduced = useReducedMotion();
  if (file === 'missing' && missing) return missing;
  if (file === undefined || file === 'missing') {
    return <span className="gif-image is-empty" style={size ? { width: size, height: size } : undefined} data-ready={false} aria-hidden />;
  }
  const box = size ? { width: size, height: size } : { width: file.info.width, height: file.info.height };
  return (
    <img
      className="gif-image"
      src={reduced ? file.poster : file.url}
      width={file.info.width}
      height={file.info.height}
      style={box}
      alt=""
      draggable={false}
      data-ready
      data-motion={reduced ? 'poster' : 'playing'}
      data-testid="gif-image"
    />
  );
}
