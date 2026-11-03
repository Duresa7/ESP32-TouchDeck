import { sameAssessment } from '../lib/device/controls.ts';
import { controlBindings } from '../lib/editor/control-bindings.ts';
import { canonical, parseLayout, type Orientation } from '../lib/editor/validate.ts';
import type { ControlLayout, DeviceSummary } from './api.ts';
import type { JobEnd } from './fake-layouts.ts';

type Frame = Record<string, unknown> & { type: string };

export interface FakeControlsOptions {
  devices: Map<string, DeviceSummary>;
  publish(frame: Frame): void;
  stored(device: string, orientation: Orientation): string | null;
  delayMs?: number;
}

const CHECKING: ControlLayout = { state: 'unavailable', reason: 'checking' };

const activeOf = (device: DeviceSummary): Orientation => (device.orientation === 'portrait' ? 'portrait' : 'landscape');

export function createFakeControls(options: FakeControlsOptions) {
  const { devices, publish, stored, delayMs = 150 } = options;
  const pushed = new Map<string, string>();
  const before = new Map<string, ControlLayout>();
  const held = new Set<string>();
  const waiting = new Map<string, () => void>();
  const slot = (device: string, orientation: Orientation) => `${device}/${orientation}`;

  function set(id: string, control_layout: ControlLayout) {
    const current = devices.get(id);
    if (!current || sameAssessment(current.control_layout, control_layout)) return;
    const next = { ...current, control_layout, rev: current.rev + 1 };
    devices.set(id, next);
    publish({ type: 'device.updated', device: id, data: next });
  }

  function assess(device: DeviceSummary): ControlLayout {
    if (device.link.state !== 'connected') return { state: 'unavailable', reason: 'offline' };
    const orientation = activeOf(device);
    const text = stored(device.id, orientation);
    const parsed = text === null ? null : parseLayout(text);
    if (parsed && !parsed.ok) return { state: 'unavailable', reason: 'read-failed' };
    const shown = parsed ? canonical(parsed.layout) : null;
    const has_controls = parsed ? controlBindings(parsed.layout, []).length > 0 : false;
    const copy = pushed.get(slot(device.id, orientation));
    if (copy === undefined) return { state: 'no-pushed-copy', orientation, has_controls };
    return { state: copy === shown ? 'matches' : 'differs', orientation, has_controls };
  }

  function settle({ device: id, orientation, outcome }: { device: string; orientation: Orientation; outcome: JobEnd }) {
    const device = devices.get(id);
    const was = before.get(id) ?? CHECKING;
    before.delete(id);
    if (!device) return;
    const fresh = (outcome === 'succeeded' && orientation === activeOf(device)) || sameAssessment(was, CHECKING);
    set(id, fresh ? assess(device) : was);
  }

  return {
    started(id: string) {
      const device = devices.get(id);
      if (!device) return;
      if (!before.has(id)) before.set(id, device.control_layout);
      set(id, CHECKING);
    },
    finished(push: { device: string; orientation: Orientation; outcome: JobEnd }) {
      const { device: id, orientation, outcome } = push;
      if (outcome === 'succeeded') {
        const text = stored(id, orientation);
        if (text !== null) pushed.set(slot(id, orientation), text);
      }
      const run = () => settle(push);
      if (held.delete(id)) waiting.set(id, run);
      else setTimeout(run, delayMs);
    },
    set,
    hold: (id: string) => void held.add(id),
    release(id: string) {
      waiting.get(id)?.();
      waiting.delete(id);
    },
  };
}

export type FakeControls = ReturnType<typeof createFakeControls>;
