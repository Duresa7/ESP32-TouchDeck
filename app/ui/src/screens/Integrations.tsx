import { useLayoutEffect, useRef, useState } from 'react';
import { Tooltip } from '@base-ui/react/tooltip';
import { useIntegrations } from '../companion/integrations.ts';
import { useAddToLayout } from './integrations/addToLayout.ts';
import { IntegrationDetail } from './integrations/IntegrationDetail.tsx';
import { IntegrationList } from './integrations/IntegrationList.tsx';
import '../styles/integrations.css';

export function Integrations() {
  const integrations = useIntegrations((s) => s.integrations);
  const [open, setOpen] = useState<{ id: string; setUp: boolean } | null>(null);
  const back = useRef<string | null>(null);
  const list = useRef<HTMLDivElement>(null);
  const addToLayout = useAddToLayout();
  const summary = open ? integrations.find((i) => i.id === open.id) : undefined;

  useLayoutEffect(() => {
    if (open || !back.current) return;
    list.current?.querySelector<HTMLElement>(`[data-id="${CSS.escape(back.current)}"] [data-testid="integration-open"]`)?.focus();
    back.current = null;
  }, [open]);

  return (
    <Tooltip.Provider delay={500} closeDelay={0}>
      <main className="integrations" data-testid="integrations">
        {summary && open ? (
          <IntegrationDetail
            key={summary.id}
            summary={summary}
            setUp={open.setUp}
            onBack={() => {
              back.current = summary.id;
              setOpen(null);
            }}
            onAdd={(preset) => void addToLayout(preset)}
          />
        ) : (
          <div ref={list}>
            <header className="page-head">
              <h1 className="page-title">Integrations</h1>
              <p className="page-lede">Connect apps and services, then add their Preset tiles to any Page.</p>
            </header>
            <IntegrationList integrations={integrations} onOpen={(id, setUp) => setOpen({ id, setUp })} onAdd={(preset) => void addToLayout(preset)} />
          </div>
        )}
      </main>
    </Tooltip.Provider>
  );
}
