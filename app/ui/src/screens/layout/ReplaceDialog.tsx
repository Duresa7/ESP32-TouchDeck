import { useRef } from 'react';
import { AlertDialog } from '@base-ui/react/alert-dialog';
import { describeLayout } from '../../lib/editor/messages.ts';
import type { EditorStore } from '../../lib/editor/store.ts';
import { button } from '../../shell/button.ts';
import { useEditorState } from './editor.ts';

/**
 * Replace or Keep mine, when Load from {device} finds a different Layout or a send finds the Device's Layout changed
 * since it was loaded (412). Keep mine after a 412 sends this Layout over the Device's; Escape keeps it without sending.
 */
export function ReplaceDialog({ store, device }: { store: EditorStore; device: string }) {
  const conflict = useEditorState(store, (s) => s.conflict);
  // The dialog keeps its words while it fades out.
  const last = useRef(conflict);
  if (conflict) last.current = conflict;
  const shown = last.current;
  const keepMine = () => {
    const changed = store.getState().conflict?.reason === 'changed';
    store.keepMine();
    if (changed) void store.push();
  };
  return (
    <AlertDialog.Root open={conflict !== null} onOpenChange={(open) => !open && conflict && store.keepMine()}>
      <AlertDialog.Portal>
        <AlertDialog.Backdrop className="scrim" />
        <AlertDialog.Popup className="dialog dialog-confirm" data-testid="replace-dialog">
          {shown && (
            <>
              <AlertDialog.Title className="dialog-title">
                {device} has “{shown.layout.name}”
              </AlertDialog.Title>
              <AlertDialog.Description className="dialog-text">
                ({describeLayout(shown.layout)}). Replace the {shown.orientation} Layout here with it?
              </AlertDialog.Description>
              <div className="dialog-actions">
                <button type="button" className={button()} onClick={keepMine} data-testid="keep-mine">
                  Keep mine
                </button>
                <button type="button" className={button({ kind: 'primary' })} onClick={() => store.replace()} data-testid="replace">
                  Replace
                </button>
              </div>
            </>
          )}
        </AlertDialog.Popup>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
