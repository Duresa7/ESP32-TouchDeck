import clsx from 'clsx';
import { Plus } from 'lucide-react';
import type { EditorStore } from '../../lib/editor/store.ts';
import { PAGES_MAX, type Layout } from '../../lib/editor/validate.ts';
import { Tip } from './Tip.tsx';

/** One tab per Page under the Device, the current one marked with the Device's own pager pill, then Add Page. */
export function PageTabs({ store, layout, page }: { store: EditorStore; layout: Layout; page: string | null }) {
  const full = layout.pages.length >= PAGES_MAX;
  const add = (
    <button
      type="button"
      className="page-tab page-add"
      aria-label="Add Page"
      aria-disabled={full}
      onClick={() => !full && store.addPage()}
      data-testid="page-add"
    >
      <Plus size={16} strokeWidth={2} aria-hidden />
      Page
    </button>
  );
  return (
    <div className="page-tabs" role="group" aria-label="Pages">
      {layout.pages.map((p) => (
        <button
          key={p.id}
          type="button"
          className={clsx('page-tab', p.id === page && 'is-current')}
          aria-current={p.id === page ? 'true' : undefined}
          onClick={() => store.selectPage(p.id)}
          data-testid="page-tab"
          data-page={p.id}
        >
          {p.title}
          {p.kind === 'dashboard' && <span className="page-tag">Usage</span>}
        </button>
      ))}
      {full ? <Tip label={`A Layout holds up to ${PAGES_MAX} Pages`}>{add}</Tip> : add}
    </div>
  );
}
