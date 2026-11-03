import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { ControlLayout, DeviceSummary, Orientation } from '../../companion/api.ts';
import { devicesStore, type DevicesStore } from '../../companion/devices.ts';

export type NeedsSend = 'no-pushed-copy' | 'differs';

export interface ControlWatch {
  needs: { reason: NeedsSend; orientation: Orientation } | null;
  send: { orientation: Orientation; phase: 'sending' | 'checking'; reset: boolean; before: ControlLayout } | null;
}

export const QUIET: ControlWatch = { needs: null, send: null };

export type SendOutcome = 'succeeded' | 'failed';

export type ControlNotice =
  | { kind: 'none' }
  | { kind: 'needs-send'; reason: NeedsSend; orientation: Orientation }
  | { kind: 'checking'; orientation: Orientation };

const NONE: ControlNotice = { kind: 'none' };

const connected = (device: Pick<DeviceSummary, 'link'>) => device.link.state === 'connected';

export function sameAssessment(a: ControlLayout, b: ControlLayout): boolean {
  if (a.state === 'unavailable') return b.state === 'unavailable' && a.reason === b.reason;
  if (b.state === 'unavailable') return false;
  return a.state === b.state && a.orientation === b.orientation && a.has_controls === b.has_controls;
}

export function observe(watch: ControlWatch, device: Pick<DeviceSummary, 'link' | 'control_layout'>): ControlWatch {
  const status = device.control_layout;
  if (!connected(device)) return QUIET;
  switch (status.state) {
    case 'matches':
      return QUIET;
    case 'unavailable':
      if (status.reason !== 'checking') return QUIET;
      return watch.send && !watch.send.reset ? { ...watch, send: { ...watch.send, reset: true } } : watch;
    case 'no-pushed-copy':
    case 'differs': {
      const needs = status.has_controls ? { reason: status.state, orientation: status.orientation } : null;
      const stale = watch.send !== null && !watch.send.reset && sameAssessment(watch.send.before, status);
      return { needs, send: null };
    }
    default: {
      const exhaustive: never = status;
      return exhaustive;
    }
  }
}

export function sending(watch: ControlWatch, { orientation, before }: { orientation: Orientation; before: ControlLayout }): ControlWatch {
  if (watch.needs?.orientation !== orientation) return watch;
  return { ...watch, send: { orientation, phase: 'sending', reset: false, before } };
}

export function sent(watch: ControlWatch, outcome: SendOutcome): ControlWatch {
  if (!watch.send) return watch;
  return outcome === 'succeeded' ? { ...watch, send: { ...watch.send, phase: 'checking' } } : { ...watch, send: null };
}

export function noticeOf(watch: ControlWatch | undefined, device: Pick<DeviceSummary, 'link'>): ControlNotice {
  if (!watch || !connected(device)) return NONE;
  if (watch.send?.phase === 'checking') return { kind: 'checking', orientation: watch.send.orientation };
  return watch.needs ? { kind: 'needs-send', ...watch.needs } : NONE;
}

export interface ControlsState {
  watched: Partial<Record<string, ControlWatch>>;
  sending(device: string, orientation: Orientation): void;
  sent(device: string, outcome: SendOutcome): void;
}

export function createControlsStore(devices: DevicesStore) {
  const store = createStore<ControlsState>()((set, get) => {
    const change = (id: string, next: (watch: ControlWatch) => ControlWatch) => {
      const watch = get().watched[id];
      if (watch) set({ watched: { ...get().watched, [id]: next(watch) } });
    };
    return {
      watched: {},
      sending: (id, orientation) => {
        const device = devices.getState().devices.find((d) => d.id === id);
        if (device) change(id, (watch) => sending(watch, { orientation, before: device.control_layout }));
      },
      sent: (id, outcome) => change(id, (watch) => sent(watch, outcome)),
    };
  });
  const take = (list: readonly DeviceSummary[], before: readonly DeviceSummary[]) => {
    const { watched } = store.getState();
    const next: Record<string, ControlWatch> = {};
    let changed = list.length !== Object.keys(watched).length;
    for (const device of list) {
      const was = watched[device.id];
      next[device.id] = was && before.includes(device) ? was : observe(was ?? QUIET, device);
      if (next[device.id] !== was) changed = true;
    }
    if (changed) store.setState({ watched: next });
  };
  take(devices.getState().devices, []);
  devices.subscribe((state, previous) => {
    if (state.devices !== previous.devices) take(state.devices, previous.devices);
  });
  return store;
}

export type ControlsStore = ReturnType<typeof createControlsStore>;

export const controlsStore = createControlsStore(devicesStore);

export function useControlNotice(device: DeviceSummary): ControlNotice {
  const watch = useStore(controlsStore, (s) => s.watched[device.id]);
  return noticeOf(watch, device);
}
