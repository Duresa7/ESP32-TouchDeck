import { useEffect, useId, useState } from 'react';
import { Switch } from '@base-ui/react/switch';
import { useStore } from 'zustand';
import { useCompanion } from '../../companion/connection.ts';
import { createStartupStore } from './startup.ts';

export function StartAtSignIn() {
  const companion = useCompanion();
  const [store] = useState(() => createStartupStore(companion));
  const view = useStore(store, (s) => s.view);
  const id = useId();
  useEffect(() => void store.getState().load(), [store]);
  const problem = view.kind === 'loading' ? null : view.problem;

  return (
    <div className="row">
      <span className="row-text">
        <span className="row-title" id={`${id}-title`}>
          Start at sign-in
        </span>
        <span className="row-sub" id={`${id}-hint`}>
          Runs in the tray when you sign in to Windows, so your Devices work right away.
        </span>
        {problem && (
          <span className="row-problem" role="alert">
            {problem}
          </span>
        )}
      </span>
      <Switch.Root
        className="settings-switch"
        checked={view.kind === 'ready' && view.on}
        disabled={view.kind !== 'ready'}
        onCheckedChange={(on) => void store.getState().set(on)}
        aria-labelledby={`${id}-title`}
        aria-describedby={`${id}-hint`}
      >
        <Switch.Thumb className="settings-switch-thumb" />
      </Switch.Root>
    </div>
  );
}
