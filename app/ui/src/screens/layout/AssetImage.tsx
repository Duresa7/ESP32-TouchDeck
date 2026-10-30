import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { Companion } from '../../companion/client.ts';
import { useCompanion } from '../../companion/connection.ts';
import { isApiError } from '../../companion/errors.ts';
import { assetOf, type ActionType, type Tile } from '../../lib/editor/validate.ts';
import { decodeTdi1, TDI1_SIDE } from '../../lib/icons/tdi1.ts';
import { TileGlyph } from './glyphs.tsx';

const pixels = new Map<string, Uint8ClampedArray | 'missing'>();

async function load(companion: Companion, id: string, signal: AbortSignal): Promise<Uint8ClampedArray | 'missing'> {
  try {
    const blob = await companion.assets.file({ asset: id }, null, null, undefined, undefined, signal);
    return decodeTdi1(new Uint8Array(await blob.arrayBuffer())) ?? 'missing';
  } catch (error) {
    if (isApiError(error, 'asset.not_found')) return 'missing';
    throw error;
  }
}

export function AssetImage({ id, size, onMissing }: { id: string; size: number; onMissing?: () => void }) {
  const companion = useCompanion();
  const canvas = useRef<HTMLCanvasElement>(null);
  const [shown, setShown] = useState(() => pixels.get(id));
  const missing = useRef(onMissing);
  missing.current = onMissing;

  useEffect(() => {
    const known = pixels.get(id);
    setShown(known);
    if (known === 'missing') missing.current?.();
    if (known !== undefined) return;
    const controller = new AbortController();
    load(companion, id, controller.signal).then(
      (result) => {
        pixels.set(id, result);
        setShown(result);
        if (result === 'missing') missing.current?.();
      },
      () => {},
    );
    return () => controller.abort();
  }, [companion, id]);

  useLayoutEffect(() => {
    const context = canvas.current?.getContext('2d');
    if (!context) return;
    context.clearRect(0, 0, TDI1_SIDE, TDI1_SIDE);
    if (shown && shown !== 'missing') context.putImageData(new ImageData(new Uint8ClampedArray(shown), TDI1_SIDE, TDI1_SIDE), 0, 0);
  }, [shown]);

  return <canvas ref={canvas} className="asset-image" width={TDI1_SIDE} height={TDI1_SIDE} style={{ width: size, height: size }} data-ready={shown !== undefined && shown !== 'missing'} aria-hidden />;
}

export function TileIcon({ tile, kind, size }: { tile: Pick<Tile, 'icon' | 'asset'>; kind: ActionType; size: number }) {
  const id = assetOf(tile) ?? (tile.icon?.startsWith('file:') ? tile.icon.slice('file:'.length) : null);
  return id ? <AssetImage key={id} id={id} size={Math.round(size * 1.5)} /> : <TileGlyph icon={tile.icon} kind={kind} size={size} />;
}
