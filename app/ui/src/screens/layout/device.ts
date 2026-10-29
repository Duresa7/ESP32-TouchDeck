import { useEffect } from 'react';
import { useStore } from 'zustand';
import { createStore, type StoreApi } from 'zustand/vanilla';
import type { DeviceSummary } from '../../companion/api.ts';
import type { Companion } from '../../companion/client.ts';
import { useCompanion } from '../../companion/connection.ts';
import { jobsStore } from '../../companion/jobs.ts';
import { createFirmwareUpdate, type FirmwareUpdateStore } from '../../lib/device/firmware.ts';
import { newerFirmware } from '../../lib/device/needs-firmware.ts';
import { createDeviceSettings, type DeviceSettingsStore } from '../../lib/device/settings.ts';

interface DeviceStores {
  settings: DeviceSettingsStore;
  firmware: FirmwareUpdateStore;
  observe(summary: DeviceSummary): void;
}

const stores = new WeakMap<Companion, Map<string, DeviceStores>>();

export function deviceStoresFor(companion: Companion, device: DeviceSummary): DeviceStores {
  let byDevice = stores.get(companion);
  if (!byDevice) {
    byDevice = new Map();
    stores.set(companion, byDevice);
  }
  let found = byDevice.get(device.id);
  if (!found) {
    let latest = device;
    const settings = createDeviceSettings({ companion, device });
    const firmware = createFirmwareUpdate({
      companion,
      jobs: jobsStore,
      context: () => ({ id: latest.id, device: latest.name ?? 'the Device', version: latest.firmware?.version ?? '' }),
    });
    found = {
      settings,
      firmware,
      observe(summary) {
        latest = summary;
        settings.observe(summary);
      },
    };
    byDevice.set(device.id, found);
  }
  return found;
}

export function useDeviceStores(device: DeviceSummary): DeviceStores {
  const stores = deviceStoresFor(useCompanion(), device);
  const job = device.link.state === 'updating' ? device.link.job : null;
  useEffect(() => stores.observe(device), [stores, device]);
  useEffect(() => {
    const known = job ? jobsStore.getState().jobs[job] : undefined;
    if (known) stores.firmware.attach(known);
  }, [stores, job]);
  return stores;
}

type Bundled = { state: 'asking' } | { state: 'known'; version: string } | { state: 'unknown' };

const bundled = new WeakMap<Companion, StoreApi<Bundled>>();

function bundledFor(companion: Companion): StoreApi<Bundled> {
  let store = bundled.get(companion);
  if (!store) {
    const asking = createStore<Bundled>()(() => ({ state: 'asking' }));
    companion.firmware.bundled().then(
      (build) => asking.setState({ state: 'known', version: build.version }),
      () => asking.setState({ state: 'unknown' }),
    );
    bundled.set(companion, asking);
    store = asking;
  }
  return store;
}

export function useBundledFirmware(): string | null {
  return useStore(bundledFor(useCompanion()), (s) => (s.state === 'known' ? s.version : null));
}

export function useOfferedFirmware(device: Pick<DeviceSummary, 'firmware'>): string | null {
  const bundled = useBundledFirmware();
  return bundled ? newerFirmware(device, bundled) : (device.firmware?.update_available ?? null);
}
