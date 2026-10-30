import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import clsx from 'clsx';
import { Switch } from '@base-ui/react/switch';
import { Plus } from 'lucide-react';
import { useIntegrations } from '../../companion/integrations.ts';
import { missingGlyphs } from '../../lib/deck/scene.ts';
import { ADD_KINDS, tileProblems, type EditorStore, type Target } from '../../lib/editor/store.ts';
import { plural } from '../../lib/editor/messages.ts';
import type { GridPage, Layout, Page } from '../../lib/editor/validate.ts';
import { offersPresets } from '../../lib/integrations/model.ts';
import { button } from '../../shell/button.ts';
import { useUi } from '../../state/ui.ts';
import { PresetButton } from '../integrations/PresetPreview.tsx';
import { PendingApprovals } from './Approvals.tsx';
import { useEditorState } from './editor.ts';
import { TileIcon } from './AssetImage.tsx';
import { KINDS } from './glyphs.tsx';
import { TileFields } from './TileFields.tsx';
import { Tip } from './Tip.tsx';

function GlyphWarning({ value, device }: { value: string; device: string }) {
  const missing = missingGlyphs(value);
  if (!missing.length) return null;
  return (
    <p className="field-hint is-warning">
      {device} can't show {missing.join(' ')} and draws an empty box instead.
    </p>
  );
}

function NameField({ label, value, hint, device, onCommit, testId }: { label: string; value: string; hint?: string; device: string; onCommit(next: string): string | null; testId: string }) {
  const id = useId();
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const commit = () => {
    if (draft === value) return;
    if (onCommit(draft) !== null) setDraft(value);
  };
  const keys = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') commit();
    if (event.key === 'Escape' && draft !== value) {
      event.stopPropagation();
      setDraft(value);
    }
  };
  return (
    <div className="field">
      <label className="field-label" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        className="input"
        value={draft}
        maxLength={24}
        spellCheck={false}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={keys}
        data-testid={testId}
      />
      {hint && <p className="field-hint">{hint}</p>}
      <GlyphWarning value={draft} device={device} />
    </div>
  );
}

const SIZES = [1, 2, 3, 4, 5, 6, 7, 8];

function GridField({ store, page }: { store: EditorStore; page: GridPage }) {
  const id = useId();
  return (
    <div className="field">
      <span className="field-label" id={id}>
        Grid
      </span>
      <div className="grid-size" role="group" aria-labelledby={id}>
        <select className="input select-native tabular" aria-label="Rows" value={page.rows} onChange={(event) => store.setGrid(Number(event.target.value), page.cols)} data-testid="grid-rows">
          {SIZES.map((n) => (
            <option key={n} value={n}>
              {plural(n, 'row')}
            </option>
          ))}
        </select>
        <span className="grid-times" aria-hidden>
          ×
        </span>
        <select className="input select-native tabular" aria-label="Columns" value={page.cols} onChange={(event) => store.setGrid(page.rows, Number(event.target.value))} data-testid="grid-cols">
          {SIZES.map((n) => (
            <option key={n} value={n}>
              {plural(n, 'column')}
            </option>
          ))}
        </select>
      </div>
      <p className="field-hint">More rows and columns make smaller Tiles. Empty squares stay blank.</p>
    </div>
  );
}

function RemovePage({ store, layout, page }: { store: EditorStore; layout: Layout; page: Page }) {
  const [armed, setArmed] = useState(false);
  useEffect(() => setArmed(false), [page.id]);
  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), 4000);
    return () => clearTimeout(timer);
  }, [armed]);
  const last = layout.pages.length <= 1;
  const control = (
    <button
      type="button"
      className={button({ kind: 'danger', size: 'small' })}
      aria-disabled={last}
      onClick={() => {
        if (last) return;
        if (armed) store.removePage(page.id);
        else setArmed(true);
      }}
      onBlur={() => setArmed(false)}
      data-testid="page-remove"
    >
      {armed ? `Click again to remove “${page.title}”` : 'Remove Page'}
    </button>
  );
  return last ? <Tip label="A Layout needs at least one Page">{control}</Tip> : control;
}

function Details({ store, layout, page, device }: { store: EditorStore; layout: Layout; page: Page; device: string }) {
  const index = layout.pages.findIndex((p) => p.id === page.id);
  const fullId = useId();
  return (
    <div className="inspector-details" data-testid="page-details">
      <section className="inspector-section" aria-labelledby={`${fullId}-page`}>
        <div className="section-head">
          <h2 className="section-title" id={`${fullId}-page`}>
            {page.kind === 'dashboard' ? 'Usage page' : 'Page'}
          </h2>
          <span className="section-sub tabular">
            Page {index + 1} of {layout.pages.length}
          </span>
        </div>
        <NameField
          key={page.id}
          label="Name"
          value={page.title}
          hint={`Shown at the top left of ${device}. Up to 24 characters.`}
          device={device}
          onCommit={(title) => store.renamePage(page.id, title)}
          testId="page-name"
        />
        {page.kind === 'grid' ? (
          <GridField store={store} page={page} />
        ) : (
          <p className="field-hint">Lists each CLIProxyAPI account and how much of its limit is used. Shows examples until CLIProxyAPI is set up under Integrations.</p>
        )}
        <div>
          <RemovePage store={store} layout={layout} page={page} />
        </div>
      </section>
      <section className="inspector-section" aria-labelledby={`${fullId}-layout`}>
        <div className="section-head">
          <h2 className="section-title" id={`${fullId}-layout`}>
            Layout
          </h2>
        </div>
        <NameField label="Name" value={layout.name} device={device} onCommit={(name) => store.renameLayout(name)} testId="layout-name" />
        <div className="field">
          <div className="switch-row">
            <span className="field-label" id={`${fullId}-full`}>
              Full screen
            </span>
            <Switch.Root
              className="switch"
              checked={layout.fullScreen === true}
              onCheckedChange={(on) => store.setFullScreen(on)}
              aria-labelledby={`${fullId}-full`}
              data-testid="full-screen"
            >
              <Switch.Thumb className="switch-thumb" />
            </Switch.Root>
          </div>
          <p className="field-hint">Hide the Page name and dots so Tiles fill the screen. Swiping between Pages still works.</p>
        </div>
      </section>
    </div>
  );
}

function TileInspector({ store, page, tileId }: { store: EditorStore; page: GridPage; tileId: string }) {
  const tile = page.tiles.find((t) => t.id === tileId);
  const refused = useEditorState(store, (s) => tileProblems(s, tileId)?.action);
  if (!tile) return null;
  const kind = refused?.value.type ?? tile.action.type;
  return (
    <>
      <div className="inspector-head" data-testid="tile-head">
        <span className="tile-thumb" aria-hidden>
          <TileIcon tile={tile} kind={kind} size={22} />
        </span>
        <div className="inspector-head-text">
          <h2 className="inspector-title" data-testid="tile-title">
            {tile.label}
          </h2>
          <p className="inspector-sub">
            {KINDS[kind].noun} on the {page.title} Page
          </p>
        </div>
      </div>
      <TileFields store={store} page={page} tile={tile} />
      <div className="inspector-foot">
        <button type="button" className={button()} onClick={() => store.duplicate(tile.id)} data-testid="tile-duplicate">
          Duplicate
        </button>
        <button type="button" className={button({ kind: 'danger' })} onClick={() => store.clearTile(tile.id)} data-testid="tile-clear">
          Clear Tile
        </button>
      </div>
    </>
  );
}

function where(target: Target, page: Page) {
  if (target === 'new-page') return `The ${page.title} Page is full, so it starts a new Page`;
  return `Row ${target.row + 1}, column ${target.col + 1} on the ${page.title} Page`;
}

function PresetTiles({ store }: { store: EditorStore }) {
  const integrations = useIntegrations((s) => s.integrations);
  const go = useUi((s) => s.go);
  const offered = integrations.filter(offersPresets);
  return (
    <section className="inspector-section" aria-label="Preset tiles" data-testid="add-presets">
      <h3 className="add-heading">Preset tiles</h3>
      {offered.length ? (
        offered.map((integration) => (
          <div className="add-presets-group" key={integration.id} data-id={integration.id}>
            <span className="add-presets-name">{integration.name}</span>
            <div className="preset-grid">
              {integration.presets.map((preset) => (
                <PresetButton key={preset.id} preset={preset} streams={integration.streams} onAdd={(chosen) => store.placePreset(chosen)} />
              ))}
            </div>
          </div>
        ))
      ) : (
        <div className="add-presets-group">
          <p className="field-hint">Set up an Integration to add its Preset tiles.</p>
          <div>
            <button type="button" className={button({ size: 'small' })} onClick={() => go('integrations')} data-testid="add-presets-setup">
              Integrations
            </button>
          </div>
        </div>
      )}
    </section>
  );
}

function AddTile({ store, page, target }: { store: EditorStore; page: Page; target: Target }) {
  const first = useRef<HTMLButtonElement>(null);
  useEffect(() => first.current?.focus({ preventScroll: true }), []);
  return (
    <>
      <div className="inspector-head">
        <span className="tile-thumb tile-thumb-add" aria-hidden>
          <Plus size={22} strokeWidth={2} />
        </span>
        <div className="inspector-head-text">
          <h2 className="inspector-title">Add Tile</h2>
          <p className="inspector-sub" data-testid="add-where">
            {where(target, page)}
          </p>
        </div>
      </div>
      <section className="inspector-section" aria-label="Action">
        <h3 className="add-heading">Action</h3>
        <div className="add-actions">
          {ADD_KINDS.map((kind, i) => {
            const Icon = KINDS[kind].icon;
            return (
              <button key={kind} ref={i === 0 ? first : undefined} type="button" className="add-action" onClick={() => store.place(kind)} data-testid="add-kind" data-kind={kind}>
                <span className="add-action-glyph" aria-hidden>
                  <Icon size={16} strokeWidth={1.9} />
                </span>
                <span className="add-action-text">
                  <span className="add-action-name">{KINDS[kind].name}</span>
                  <span className="add-action-line">{KINDS[kind].line}</span>
                </span>
              </button>
            );
          })}
        </div>
      </section>
      <PresetTiles store={store} />
      <div className="inspector-foot">
        <button type="button" className={button()} onClick={() => store.closeAdd()} data-testid="add-cancel">
          Cancel
        </button>
      </div>
    </>
  );
}

export function Inspector({ store, layout, device }: { store: EditorStore; layout: Layout; device: string }) {
  const pageId = useEditorState(store, (s) => s.page);
  const selected = useEditorState(store, (s) => s.selected);
  const adding = useEditorState(store, (s) => s.adding);
  const mode = useEditorState(store, (s) => s.mode);
  const deviceId = useEditorState(store, (s) => s.device?.id);
  const page = layout.pages.find((p) => p.id === pageId) ?? layout.pages[0];
  const tileOpen = mode === 'edit' && selected !== null && page.kind === 'grid' && page.tiles.some((t) => t.id === selected);
  return (
    <aside className={clsx('inspector', adding !== null && 'is-adding')} aria-label="Details" data-testid="inspector">
      {deviceId && <PendingApprovals deviceId={deviceId} device={device} layout={layout} />}
      {adding !== null ? (
        <AddTile store={store} page={page} target={adding} />
      ) : tileOpen && page.kind === 'grid' ? (
        <TileInspector store={store} page={page} tileId={selected!} />
      ) : (
        <Details store={store} layout={layout} page={page} device={device} />
      )}
    </aside>
  );
}
