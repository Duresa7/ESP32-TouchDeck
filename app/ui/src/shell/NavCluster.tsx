import type { ComponentType, ReactElement } from 'react';
import clsx from 'clsx';
import { Dialog } from '@base-ui/react/dialog';
import { Tooltip } from '@base-ui/react/tooltip';
import { Cable, House, LayoutGrid, Settings, type LucideProps } from 'lucide-react';
import { useUi, type Section } from '../state/ui.ts';
import { SettingsDialog } from './SettingsDialog.tsx';
import { UpdateSlot } from './UpdateSlot.tsx';

const destinations: { section: Section; label: string; icon: ComponentType<LucideProps> }[] = [
  { section: 'home', label: 'Home', icon: House },
  { section: 'layout', label: 'Layout', icon: LayoutGrid },
  { section: 'integrations', label: 'Integrations', icon: Cable },
];

function NavButton({
  label,
  icon: Icon,
  current,
  page,
  onClick,
  render,
}: {
  label: string;
  icon: ComponentType<LucideProps>;
  current: boolean;
  /** A destination marks itself as the current page; Settings only looks current while open. */
  page?: boolean;
  onClick?: () => void;
  render?: ReactElement;
}) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger
        render={render}
        className={clsx('nav-button', current && 'is-current')}
        aria-label={label}
        aria-current={page && current ? 'page' : undefined}
        onClick={onClick}
      >
        <Icon size={18} strokeWidth={1.9} aria-hidden />
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Positioner side="top" sideOffset={8}>
          <Tooltip.Popup className="tooltip">{label}</Tooltip.Popup>
        </Tooltip.Positioner>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}

/**
 * The bottom-left cluster: Home, Layout, Integrations and Settings, then the
 * Update slot in its own puck. Names appear as tooltips after 500ms, and at
 * once while the pointer stays in the cluster.
 */
export function NavCluster() {
  const section = useUi((s) => s.section);
  const go = useUi((s) => s.go);
  const settingsOpen = useUi((s) => s.settingsOpen);
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);

  return (
    <Tooltip.Provider delay={500} closeDelay={0}>
      <nav className="nav-dock" aria-label="Main">
        <div className="nav-puck">
          {destinations.map((d) => (
            <NavButton key={d.section} label={d.label} icon={d.icon} current={section === d.section} page onClick={() => go(d.section)} />
          ))}
          <Dialog.Root open={settingsOpen} onOpenChange={setSettingsOpen}>
            <NavButton label="Settings" icon={Settings} current={settingsOpen} render={<Dialog.Trigger />} />
            <SettingsDialog />
          </Dialog.Root>
        </div>
        <UpdateSlot />
      </nav>
    </Tooltip.Provider>
  );
}
