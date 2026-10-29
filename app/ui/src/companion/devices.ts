import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { DeviceSummary, Job } from './api.ts';

export type StreamStatus = 'connecting' | 'live' | 'reconnecting' | 'unpaired';

export interface DevicesState {
  status: StreamStatus;
  devices: DeviceSummary[];
  layoutPushes: Record<string, string>;
  setStatus(status: StreamStatus): void;
  sync(devices: DeviceSummary[], jobs?: Job[]): void;
  upsert(device: DeviceSummary): void;
  remove(id: string): void;
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
