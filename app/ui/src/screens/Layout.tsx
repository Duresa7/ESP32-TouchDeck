import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import clsx from 'clsx';
import { Switch } from '@base-ui/react/switch';
import { Tooltip } from '@base-ui/react/tooltip';
import type { DeviceSummary } from '../companion/api.ts';
import { useCompanion } from '../companion/connection.ts';
import { useDevices } from '../companion/devices.ts';
import type { EditorStore } from '../lib/editor/store.ts';
import { DeviceCanvas, deviceAspect } from './layout/DeviceCanvas.tsx';
import { editorFor, useEditorState } from './layout/editor.ts';
import { EditorHeader, statusOf } from './layout/EditorHeader.tsx';
import { exportLayout, importLayout } from './layout/files.ts';
import { Inspector } from './layout/Inspector.tsx';
import { PageTabs } from './layout/PageTabs.tsx';
import { ReplaceDialog } from './layout/ReplaceDialog.tsx';
import { StartLayout } from './layout/StartLayout.tsx';
import { StatusLine } from './layout/StatusLine.tsx';
import '../styles/layout.css';

/** The Device's long side fits the canvas between these, as docs/design.md asks. */
const LONG_MIN = 420;
const LONG_MAX = 680;
/** What the canvas keeps for its padding, the Page tabs and the status line. */
const RESERVED_W = 64;
const RESERVED_H = 212;

const editable = 'input, textarea, select, [contenteditable], [role="dialog"], [role="alertdialog"], [role="menu"]';

/** Keys for the whole screen: P for Preview, ← and → change Pages in Preview, Escape and Delete in Edit. */
function useEditorKeys(store: EditorStore) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey) return;
      const target = event.target instanceof HTMLElement ? event.target : null;
      if (target?.closest(editable)) return;
      const state = store.getState();
      const layout = state.drafts[state.orientation]?.layout;
      if (!layout) return;
      if (event.key === 'p' || event.key === 'P') {
        store.setMode(state.mode === 'edit' ? 'preview' : 'edit');
      } else if (state.mode === 'preview' && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
        if (target && target !== document.body && !target.closest('.canvas')) return;
        event.preventDefault();
        const index = layout.pages.findIndex((p) => p.id === state.page);
        const next = layout.pages[index + (event.key === 'ArrowLeft' ? -1 : 1)];
        if (next) store.selectPage(next.id);
      } else if (event.key === 'Escape') {
        if (state.adding) store.closeAdd();
        else if (state.selected) store.select(null);
      } else if ((event.key === 'Delete' || event.key === 'Backspace') && state.mode === 'edit' && state.selected) {
        event.preventDefault();
        store.clearTile(state.selected);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [store]);
}

/** The Device's long side for the canvas's size and the Layout's orientation. */
function useDeviceLong(canvas: RefObject<HTMLDivElement | null>, portrait: boolean) {
  const [long, setLong] = useState(LONG_MAX);
  useLayoutEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const fit = () => {
      const aspect = deviceAspect();
      const w = el.clientWidth - RESERVED_W;
      const h = el.clientHeight - RESERVED_H;
      const fitted = portrait ? Math.min(h, w / aspect) : Math.min(w, h / aspect);
      setLong(Math.round(Math.min(LONG_MAX, Math.max(LONG_MIN, fitted))));
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(el);
    return () => observer.disconnect();
  }, [canvas, portrait]);
  return long;
}

function Editor({ store, device }: { store: EditorStore; device: DeviceSummary }) {
  const name = device.name ?? 'Unnamed Device';
  const connected = statusOf(device).live;
  const layout = useEditorState(store, (s) => s.drafts[s.orientation]?.layout ?? null);
  const page = useEditorState(store, (s) => s.page);
  const mode = useEditorState(store, (s) => s.mode);
  const selected = useEditorState(store, (s) => s.selected);
  const adding = useEditorState(store, (s) => s.adding);
  const [actualSize, setActualSize] = useState(false);
  const canvas = useRef<HTMLDivElement>(null);
  const picker = useRef<HTMLInputElement>(null);
  const long = useDeviceLong(canvas, layout?.orientation === 'portrait');
  useEditorKeys(store);

  const current = layout?.pages.find((p) => p.id === page) ?? layout?.pages[0];
  const preview = mode === 'preview';
  const openPicker = () => picker.current?.click();

  return (
    <main className={clsx('editor', !layout && 'is-empty')} data-testid="editor">
      <h1 className="visually-hidden">Layout</h1>
      <EditorHeader store={store} device={device} onImport={openPicker} onExport={() => void exportLayout(store)} />
      <div ref={canvas} className={clsx('canvas', actualSize && preview && 'is-actual')}>
        {!connected && (
          <p className="editor-banner" data-testid="disconnected">
            <strong>{name} isn't connected.</strong> Plug it in with a USB cable. You can still edit its Layout.
          </p>
        )}
        {layout && current ? (
          <>
            <DeviceCanvas
              store={store}
              layout={layout}
              pageId={current.id}
              mode={mode}
              selected={selected}
              adding={adding}
              long={long}
              actualSize={preview && actualSize}
            />
            <div className="canvas-foot">
              <PageTabs store={store} layout={layout} page={current.id} />
              {preview ? (
                <div className="preview-bar">
                  <p className="canvas-hint">Swipe or press ← and → to change Pages. Tap a Tile to see it react.</p>
                  <label className="switch-label">
                    <Switch.Root className="switch" checked={actualSize} onCheckedChange={setActualSize} data-testid="actual-size">
                      <Switch.Thumb className="switch-thumb" />
                    </Switch.Root>
                    Actual size
                  </label>
                </div>
              ) : (
                current.kind === 'dashboard' && <p className="canvas-hint">A Usage page lists your accounts once CLIProxyAPI is set up. These rows are examples.</p>
              )}
              <StatusLine store={store} />
            </div>
          </>
        ) : (
          <>
            <StartLayout store={store} device={name} connected={connected} onImport={openPicker} />
            <StatusLine store={store} />
          </>
        )}
      </div>
      {layout && <Inspector store={store} layout={layout} device={name} />}
      <ReplaceDialog store={store} device={name} />
      <input
        ref={picker}
        type="file"
        accept=".json,application/json"
        hidden
        onChange={(event) => {
          const file = event.currentTarget.files?.[0];
          event.currentTarget.value = '';
          if (file) void importLayout(store, file);
        }}
        data-testid="import-file"
      />
    </main>
  );
}

/** The Layout screen: one Device's Layout on its canvas, with its Pages, the inspector and Send to Device. */
export function Layout() {
  const companion = useCompanion();
  const store = editorFor(companion);
  const device = useDevices((s) => s.devices.find((d) => d.name !== null));
  const id = device?.id;
  const name = device?.name;
  const orientation = device?.orientation;

  useLayoutEffect(() => {
    if (id && name) store.open({ id, name }, orientation === 'portrait' ? 'portrait' : 'landscape');
  }, [store, id, name, orientation]);

  if (!device) {
    return (
      <main className="screen">
        <header className="page-head">
          <h1 className="page-title">Layout</h1>
        </header>
        <div className="empty empty-start">
          <h2 className="empty-title">Plug in your Device</h2>
          <p className="empty-text">Connect an Open TouchDeck to this PC with a USB data cable.</p>
        </div>
      </main>
    );
  }
  return (
    <Tooltip.Provider delay={500} closeDelay={0}>
      <Editor store={store} device={device} />
    </Tooltip.Provider>
  );
}
