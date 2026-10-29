import type { ReactElement, ReactNode } from 'react';
import { Tooltip } from '@base-ui/react/tooltip';

/** A control with its tooltip. The control is rendered as the trigger, so it keeps its own props. */
export function Tip({ label, side = 'bottom', children }: { label: ReactNode; side?: 'top' | 'bottom' | 'left' | 'right'; children: ReactElement }) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger render={children} />
      <Tooltip.Portal>
        <Tooltip.Positioner side={side} sideOffset={8}>
          <Tooltip.Popup className="tooltip">{label}</Tooltip.Popup>
        </Tooltip.Positioner>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}
