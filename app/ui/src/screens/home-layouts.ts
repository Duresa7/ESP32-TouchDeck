import { useEffect } from 'react';
import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { DeviceSummary, Layout, Orientation } from '../lib/companion/api.ts';
import type { Companion } from '../lib/companion/client.ts';
import { useCompanion } from '../lib/companion/connection.ts';
import { useDevices } from '../lib/companion/devices.ts';
import { isApiError } from '../lib/companion/errors.ts';
import { shownOrientation } from './home-model.ts';

const layoutsStore = createStore<{ seen: Record<string, Layout | null> }>()(() => ({ seen: {} }));

const other = (orientation: Orientation): Orientation => orientation === 'landscape' ? 'portrait' : 'landscape';

/**
 * The PC's copy of an offline Device's Layout (`layouts.last_seen`), for the orientation Home
 * shows, or the other one when the PC only saw that. Null when the PC has neither.
 */
export async function lastSeenLayout(companion: Companion, device: string, orientation: Orientation): Promise<Layout | null> {
  for (const wanted of [orientation, other(orientation)]) {
    try {
      return (await companion.layouts.last_seen({ device, orientation: wanted })).layout;
    } catch (error: unknown) {
      if (!isApiError(error, 'layout.not_found')) throw error;
    }
  }
  return null;
}

/**
 * The stored Layout the Device shows: null when it has none, undefined until read. While the
 * Device isn't connected, the Layout this PC last saw for it.
 */
export function useDeviceLayout(device: DeviceSummary): Layout | null | undefined {
  const companion = useCompanion();
  const orientation = shownOrientation(device);
  const shown = device.layout && `${device.layout.source}/${device.layout.pages}/${device.layout.name}`;
  const key = `${device.id}/${orientation}`;
  const connected = device.link.state === 'connected' ? device.link.since_ms : null;
  const pushed = useDevices((s) => s.layoutPushes[device.id]);
  useEffect(() => {
    let current = true;
    const keep = (layout: Layout | null) => {
      if (current) layoutsStore.setState((s) => ({ seen: { ...s.seen, [key]: layout } }));
    };
    if (connected === null) {
      lastSeenLayout(companion, device.id, orientation).then(
        // A Layout this window read while the Device was connected beats an empty PC copy.
        (layout) => { if (layout || layoutsStore.getState().seen[key] === undefined) keep(layout); },
        () => {},
      );
      return () => { current = false; };
    }
    companion.layouts.get({ device: device.id, orientation }).then(
      (read) => keep(read.layout),
      (error: unknown) => { if (isApiError(error, 'layout.not_found')) keep(null); },
    );
    return () => { current = false; };
  }, [companion, device.id, orientation, key, connected, pushed, shown]);
  return useStore(layoutsStore, (s) => s.seen[key]);
}
