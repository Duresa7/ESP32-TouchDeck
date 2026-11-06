import { useId, useRef, type RefObject } from 'react';
import clsx from 'clsx';
import { Toggle } from '@base-ui/react/toggle';
import { ToggleGroup } from '@base-ui/react/toggle-group';
import { Film } from 'lucide-react';
import { fittedText } from '../../lib/editor/gifs.ts';
import { gifFit, gifWork, type GifTiles, type GifWork } from '../../lib/editor/gifs.ts';
import type { EditorStore } from '../../lib/editor/store.ts';
import { assetOf, type Tile } from '../../lib/layout/model.ts';
import { button } from '../../shell/button.ts';
import { useEditorState, useGifState } from './editor.ts';
import { GifImage, useGifFile } from './GifImage.tsx';

export interface GifStatusProps {
  work: GifWork | undefined;
  fitted: string | null;
  onCancel(): void;
  onRetry(): void;
}

/** Where a Tile's GIF stands: waiting, preparing, fitted, or why it failed. */
export function GifStatus({ work, fitted, onCancel, onRetry }: GifStatusProps) {
  const state = work?.state ?? (fitted ? 'ready' : 'idle');
  return (
    <div className="gif-status" role="status" data-state={state} data-testid="gif-status">
      {(work?.state === 'queued' || work?.state === 'preparing') && (
        <div className="gif-status-row">
          <p className="field-hint">{work.state === 'queued' ? 'Waiting for another GIF…' : 'Preparing GIF…'}</p>
          <button type="button" className={button({ size: 'small' })} onClick={onCancel} data-testid="gif-cancel">
            Cancel
          </button>
        </div>
      )}
      {work?.state === 'failed' && (
        <div className="icon-problem" data-testid="gif-error">
          <p className="field-error">{work.text}</p>
          {(work.retry || work.detail) && (
            <div className="icon-problem-actions">
              {work.retry && (
                <button type="button" className={button({ size: 'small' })} onClick={onRetry} data-testid="gif-retry">
                  Retry
                </button>
              )}
              {work.detail && (
                <details className="icon-details">
                  <summary>Details</summary>
                  <p>{work.detail}</p>
                </details>
              )}
            </div>
          )}
        </div>
      )}
      {!work && fitted && (
        <p className="field-hint" data-testid="gif-fitted">
          {fitted}
        </p>
      )}
    </div>
  );
}

/** Opens the file dialog for a GIF and hands the chosen file on. */
export function GifInput({ onFile, testId, inputRef }: { onFile(file: File): void; testId: string; inputRef: RefObject<HTMLInputElement | null> }) {
  return (
    <input
      ref={inputRef}
      type="file"
      accept="image/gif"
      hidden
      onChange={(event) => {
        const chosen = event.target.files?.[0];
        event.target.value = '';
        if (chosen) onFile(chosen);
      }}
      data-testid={testId}
    />
  );
}

/** A GIF Tile's GIF: what it is, Change GIF, how it is fitted and how its preparation goes. */
export function GifField({ store, gifs, tile, device }: { store: EditorStore; gifs: GifTiles; tile: Tile; device: string }) {
  const label = useId();
  const fitLabel = useId();
  const file = useRef<HTMLInputElement>(null);
  const open = useEditorState(store, (s) => s.device);
  const orientation = useEditorState(store, (s) => s.orientation);
  const where = { device: open, orientation };
  const asset = tile.render === 'gif' ? assetOf(tile) : null;
  const work = useGifState(gifs, (s) => gifWork(s, where, tile.id));
  const fit = useGifState(gifs, (s) => gifFit(s, where, tile));
  const known = useGifState(gifs, (s) => (asset ? s.prepared[asset]?.file : undefined));
  const shown = useGifFile(asset);
  const name = work?.file || known || (asset ? 'GIF' : null);
  const fitted = shown && shown !== 'missing' ? fittedText(device, shown.info) : null;
  return (
    <>
      <div className="field" data-testid="gif-field">
        <span className="field-label" id={label}>
          GIF
        </span>
        <div className="icon-field">
          <span className="icon-chip gif-chip" aria-hidden>
            {asset ? <GifImage key={asset} id={asset} size={36} /> : <Film size={18} strokeWidth={1.9} />}
          </span>
          <span className={clsx('icon-name', !name && 'is-empty')} title={name ?? undefined} data-testid="gif-name">
            {name ?? 'No GIF'}
          </span>
          <button type="button" className={button({ size: 'small' })} aria-describedby={label} onClick={() => file.current?.click()} data-testid="gif-choose">
            {asset ? 'Change GIF' : 'Choose GIF'}
          </button>
          <GifInput inputRef={file} onFile={(chosen) => gifs.choose(tile.id, chosen)} testId="gif-file" />
        </div>
        <GifStatus work={work} fitted={fitted} onCancel={() => gifs.cancel(tile.id)} onRetry={() => gifs.retry(tile.id)} />
      </div>
      <div className="field">
        <span className="field-label" id={fitLabel}>
          Fit
        </span>
        <ToggleGroup
          value={[fit]}
          onValueChange={(value) => {
            const next = value[0];
            if (next === 'contain' || next === 'cover') gifs.setFit(tile.id, next);
          }}
          aria-labelledby={fitLabel}
          className="segmented segmented-fill"
        >
          <Toggle value="contain" className="segment" data-testid="gif-fit-contain">
            Whole GIF
          </Toggle>
          <Toggle value="cover" className="segment" data-testid="gif-fit-cover">
            Fill Tile
          </Toggle>
        </ToggleGroup>
      </div>
    </>
  );
}
