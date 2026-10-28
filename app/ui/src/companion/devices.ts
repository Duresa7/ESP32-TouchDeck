import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { DeviceSummary } from './api.ts';

/** How the event stream stands: no answer yet, live, retrying, or this browser needs to pair. */
export type StreamStatus = 'connecting' | 'live' | 'reconnecting' | 'unpaired';

export interface DevicesState {
  status: StreamStatus;
  /** Every known Device, in the order the Companion listed them. */
  devices: DeviceSummary[];
  setStatus(status: StreamStatus): void;
  /** `sync`: the Companion's snapshot replaces whatever the client had. */
  sync(devices: DeviceSummary[]): void;
  /** `device.added` and `device.updated`: a full object, ignored unless its `rev` is newer. */
  upsert(device: DeviceSummary): void;
  /** `device.removed` */
  remove(id: string): void;
}

export function createDevicesStore() {
  return createStore<DevicesState>()((set) => ({
    status: 'connecting',
    devices: [],
    setStatus: (status) => set({ status }),
    sync: (devices) => set({ devices, status: 'live' }),
    upsert: (device) =>
      set((state) => {
        const at = state.devices.findIndex((d) => d.id === device.id);
        if (at < 0) return { devices: [...state.devices, device] };
        if (state.devices[at].rev >= device.rev) return state;
        return { devices: state.devices.with(at, device) };
      }),
    remove: (id) => set((state) => ({ devices: state.devices.filter((d) => d.id !== id) })),
  }));
}

export type DevicesStore = ReturnType<typeof createDevicesStore>;

export const devicesStore = createDevicesStore();

export function useDevices<T>(select: (state: DevicesState) => T): T {
  return useStore(devicesStore, select);
}
