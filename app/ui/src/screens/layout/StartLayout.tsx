import type { ComponentType } from 'react';
import { FileUp, LayoutGrid, Sparkles, Usb, type LucideProps } from 'lucide-react';
import type { EditorStore } from '../../lib/editor/store.ts';
import { useEditorState } from './editor.ts';

function Choice({ icon: Icon, name, line, disabled, onClick, testId }: { icon: ComponentType<LucideProps>; name: string; line?: string; disabled?: boolean; onClick(): void; testId: string }) {
  return (
    <button type="button" className="start-choice" aria-disabled={disabled} onClick={() => !disabled && onClick()} data-testid={testId}>
      <span className="start-glyph" aria-hidden>
        <Icon size={18} strokeWidth={1.9} />
      </span>
      <span className="start-text">
        <span className="start-name">{name}</span>
        {line && <span className="start-line">{line}</span>}
      </span>
    </button>
  );
}

/** The empty state: no draft for this Device and orientation yet. */
export function StartLayout({ store, device, connected, onImport }: { store: EditorStore; device: string; connected: boolean; onImport(): void }) {
  const busy = useEditorState(store, (s) => s.busy);
  return (
    <div className="start" data-testid="start-layout">
      <h2 className="empty-title">Start a Layout</h2>
      <p className="empty-text">Choose what each Tile on {device} does.</p>
      <div className="start-choices">
        <Choice
          icon={Usb}
          name={busy === 'load' ? 'Loading…' : `Load from ${device}`}
          line={connected ? `Use the Layout ${device} has now.` : `Plug in ${device} to load its Layout.`}
          disabled={!connected || busy !== null}
          onClick={() => void store.load()}
          testId="start-pull"
        />
        <Choice icon={Sparkles} name="Starter Layout" line="Media controls and everyday shortcuts, ready to change." onClick={() => store.startStarter()} testId="start-starter" />
        <Choice icon={LayoutGrid} name="Blank Layout" line="One empty Page with room for 12 Tiles." onClick={() => store.startBlank()} testId="start-new" />
        <Choice icon={FileUp} name="Import" onClick={onImport} testId="start-import" />
      </div>
    </div>
  );
}
