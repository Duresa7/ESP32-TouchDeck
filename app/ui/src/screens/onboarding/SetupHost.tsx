import { lazy, Suspense, useEffect, useState } from 'react';
import { useDevices } from '../../companion/devices.ts';
import { setupCandidate } from '../../lib/device/setup-trigger.ts';
import { setupUi, useSetupUi } from '../../state/setup.ts';
import { useUi } from '../../state/ui.ts';

const Onboarding = lazy(() => import('./Onboarding.tsx').then((screen) => ({ default: screen.Onboarding })));

export function SetupHost() {
  const open = useSetupUi((s) => s.open);
  const skipped = useSetupUi((s) => s.skipped);
  const candidate = useDevices((s) => setupCandidate(s.devices, skipped)?.id ?? null);
  const [shown, setShown] = useState<string | null>(null);
  const device = useDevices((s) => s.devices.find((d) => d.id === shown));
  const openDevice = useUi((s) => s.openDevice);

  useEffect(() => {
    if (open === null && candidate !== null) setupUi.getState().start(candidate);
  }, [open, candidate]);

  useEffect(() => {
    if (open !== null) setShown(open);
  }, [open]);

  useEffect(() => {
    if (shown !== null && !device) {
      setShown(null);
      if (open === shown) setupUi.getState().close(undefined);
    }
  }, [shown, device, open]);

  if (!device) return null;
  return (
    <Suspense fallback={null}>
      <Onboarding
        key={device.id}
        device={device}
        open={open === device.id}
        onClose={(then) => {
          setupUi.getState().close(device);
          if (then === 'edit') openDevice(device.id);
        }}
        onClosed={() => setShown(null)}
      />
    </Suspense>
  );
}
