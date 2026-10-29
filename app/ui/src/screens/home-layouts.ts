import { useEffect } from 'react';
import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { DeviceSummary, Layout } from '../companion/api.ts';
import { useCompanion } from '../companion/connection.ts';
import { useDevices } from '../companion/devices.ts';
import { isApiError } from '../companion/errors.ts';
import { orientationOf } from './home-model.ts';

// The Layout each Device stores, as `layouts.get` last returned it, by `<device>/<orientation>`.
// `null` is an empty slot. The Companion reads Layouts only from a connected Device, so keeping
// them here is what lets an offline Device go on showing its saved Page.
const layoutsStore = createStore<{ seen: Record<string, Layout | null> }>()(() => ({ seen: {} }));

/**
 * The Layout a Device stores for its orientation: `undefined` until known, `null` when it stores
 * none. Read again when the Device connects, turns, or a `layouts.push` Job succeeds on it.
 */
export function useDeviceLayout(device: DeviceSummary): Layout | null | undefined {
  const companion = useCompanion();
  const orientation = orientationOf(device);
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
      // Any other failure, such as the link dropping mid-read, keeps what was last seen.
      (error: unknown) => { if (isApiError(error, 'layout.not_found')) keep(null); },
    );
    return () => { current = false; };
  }, [companion, device.id, orientation, key, connected, pushed]);
  return useStore(layoutsStore, (s) => s.seen[key]);
}
