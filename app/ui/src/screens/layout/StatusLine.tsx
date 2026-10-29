import clsx from 'clsx';
import type { EditorStore } from '../../lib/editor/store.ts';
import { useEditorState } from './editor.ts';

export function StatusLine({ store }: { store: EditorStore }) {
  const status = useEditorState(store, (s) => s.status);
  return (
    <div
      className={clsx('editor-status', status && `is-${status.kind}`)}
      role="status"
      data-testid="status"
      data-event={status?.event}
      data-kind={status?.kind}
      data-pages={status?.pages}
      data-tiles={status?.tiles}
    >
      {status && (
        <>
          <span className="editor-status-text">{status.text}</span>
          {status.kind === 'error' && status.detail && (
            <details className="editor-status-details">
              <summary>Details</summary>
              <p data-testid="status-detail">{status.detail}</p>
            </details>
          )}
        </>
      )}
    </div>
  );
}
