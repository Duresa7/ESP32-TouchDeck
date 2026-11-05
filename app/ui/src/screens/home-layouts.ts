import { useEffect } from 'react';
import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { DeviceSummary, Layout } from '../companion/api.ts';
import { useCompanion } from '../companion/connection.ts';
import { useDevices } from '../companion/devices.ts';
import { isApiError } from '../companion/errors.ts';
import { shownOrientation } from './home-model.ts';

const layoutsStore = createStore<{ seen: Record<string, Layout | null> }>()(() => ({ seen: {} }));

/** The stored Layout the Device shows: null when it has none, undefined until read. */
export function useDeviceLayout(device: DeviceSummary): Layout | null | undefined {
  const companion = useCompanion();
  const orientation = shownOrientation(device);
  const shown = device.layout && `${device.layout.source}/${device.layout.pages}/${device.layout.name}`;
  const key = `${device.id}/${orientation}`;
  const connected = device.link.state === 'connected' ? device.link.since_ms : null;
  const pushed = useDevices((s) => s.layoutPushes[device.id]);
  useEffect(() => {
    if (connected === null) return;
    let current = true;
    const keep = (layout: Layout | null) => {
      if (current) layoutsStore.setState((s) => ({ seen: { ...s.seen, [key]: layout } }));
    };
    companion.layouts.get({ device: device.id, orientation }).then(
      (read) => keep(read.layout),
      (error: unknown) => { if (isApiError(error, 'layout.not_found')) keep(null); },
    );
    return () => { current = false; };
  }, [companion, device.id, orientation, key, connected, pushed, shown]);
  return useStore(layoutsStore, (s) => s.seen[key]);
}
