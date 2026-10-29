import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { DeviceSummary, Job } from './api.ts';

/** How the event stream stands: no answer yet, live, retrying, or this browser needs to pair. */
export type StreamStatus = 'connecting' | 'live' | 'reconnecting' | 'unpaired';

export interface DevicesState {
  status: StreamStatus;
  /** Every known Device, in the order the Companion listed them. */
  devices: DeviceSummary[];
  /** The id of the last `layouts.push` Job that succeeded on each Device, so views of its Layout know to read it again. */
  layoutPushes: Record<string, string>;
  setStatus(status: StreamStatus): void;
  /** `sync`: the Companion's snapshot replaces whatever the client had. */
  sync(devices: DeviceSummary[], jobs?: Job[]): void;
  /** `device.added` and `device.updated`: a full object, ignored unless its `rev` is newer. */
  upsert(device: DeviceSummary): void;
  /** `device.removed` */
  remove(id: string): void;
  /** `job.updated` */
  job(job: Job): void;
}

const pushedTo = (job: Job) =>
  job.kind === 'layouts.push' && job.state === 'succeeded' && job.scope.kind === 'device' ? job.scope.id : null;

export function createDevicesStore() {
  return createStore<DevicesState>()((set) => ({
    status: 'connecting',
    devices: [],
    layoutPushes: {},
    setStatus: (status) => set({ status }),
    sync: (devices, jobs = []) => {
      const layoutPushes: Record<string, string> = {};
      for (const job of jobs.toSorted((a, b) => (a.finished_ms ?? 0) - (b.finished_ms ?? 0))) {
        const device = pushedTo(job);
        if (device) layoutPushes[device] = job.id;
      }
      set({ devices, layoutPushes, status: 'live' });
    },
    upsert: (device) =>
      set((state) => {
        const at = state.devices.findIndex((d) => d.id === device.id);
        if (at < 0) return { devices: [...state.devices, device] };
        if (state.devices[at].rev >= device.rev) return state;
        return { devices: state.devices.with(at, device) };
      }),
    remove: (id) => set((state) => ({ devices: state.devices.filter((d) => d.id !== id) })),
    job: (job) =>
      set((state) => {
        const device = pushedTo(job);
        return device ? { layoutPushes: { ...state.layoutPushes, [device]: job.id } } : state;
      }),
  }));
}

export type DevicesStore = ReturnType<typeof createDevicesStore>;

export const devicesStore = createDevicesStore();

export function useDevices<T>(select: (state: DevicesState) => T): T {
  return useStore(devicesStore, select);
}
