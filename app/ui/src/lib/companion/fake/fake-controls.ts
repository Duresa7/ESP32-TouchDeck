import { sameAssessment } from '../../device/controls.ts';
import { canonical, parseLayout, type Layout, type Orientation, type Tile } from '../../layout/model.ts';
import type { Capabilities, ControlLayout, DeviceSummary } from '../api.ts';

type Frame = Record<string, unknown> & { type: string };

export type Settable = (key: { integration: string; stream: string }) => boolean;

export interface FakeControlsOptions {
  devices: Map<string, DeviceSummary>;
  publish(frame: Frame): void;
  stored(device: string, orientation: Orientation): string | null;
  settable: Settable;
  delayMs?: number;
}

const CHECKING: ControlLayout = { state: 'unavailable', reason: 'checking' };

const activeOf = (device: DeviceSummary): Orientation => (device.orientation === 'portrait' ? 'portrait' : 'landscape');

const takesLive = (caps: Capabilities | null) => caps !== null && caps.live_hz > 0 && caps.live_total_hz > 0;

function drivesIntegration(tile: Tile, settable: Settable): boolean {
  const { action } = tile;
  if (action.type !== 'widget' || action.source === '') return false;
  switch (tile.render) {
    case 'now-playing':
      return true;
    case 'bar':
      return tile.streams.muted !== undefined || (tile.input === true && Object.keys(tile.streams).some((stream) => settable({ integration: action.source, stream: stream === 'value' ? action.field : stream })));
    case 'graph':
    case 'gif':
    case undefined:
      return false;
    default: {
      const exhaustive: never = tile;
      return exhaustive;
    }
  }
}

export function hasControls(layout: Layout, settable: Settable): boolean {
  return layout.pages.some((page) => page.kind === 'grid' && page.tiles.some((tile) => drivesIntegration(tile, settable)));
}

export function createFakeControls(options: FakeControlsOptions) {
  const { devices, publish, stored, settable, delayMs = 150 } = options;
  const pushed = new Map<string, string>();
  const links = new Map([...devices].map(([id, device]) => [id, JSON.stringify(device.link)]));
  const held = new Set<string>();
  const waiting = new Map<string, () => void>();
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const slot = (device: string, orientation: Orientation) => `${device}/${orientation}`;
  const read = (device: string, orientation: Orientation) => {
    const text = stored(device, orientation);
    return text === null ? null : parseLayout(text);
  };

  function set(id: string, control_layout: ControlLayout) {
    const current = devices.get(id);
    if (!current || sameAssessment(current.control_layout, control_layout)) return;
    const next = { ...current, control_layout, rev: current.rev + 1 };
    devices.set(id, next);
    publish({ type: 'device.updated', device: id, data: next });
  }

  function assess(device: DeviceSummary): ControlLayout {
    const { link } = device;
    if (link.state !== 'connected') return { state: 'unavailable', reason: 'offline' };
    if (link.protocol !== 2 || !takesLive(device.capabilities)) return { state: 'unavailable', reason: 'unsupported' };
    const orientation = activeOf(device);
    const shown = read(device.id, orientation);
    if (shown && !shown.ok) return { state: 'unavailable', reason: 'read-failed' };
    const has_controls = shown ? hasControls(shown.layout, settable) : false;
    const copy = pushed.get(slot(device.id, orientation));
    if (copy === undefined) return { state: 'no-pushed-copy', orientation, has_controls };
    return { state: shown && copy === canonical(shown.layout) ? 'matches' : 'differs', orientation, has_controls };
  }

  function reassess(id: string) {
    clearTimeout(timers.get(id));
    timers.delete(id);
    const run = () => {
      timers.delete(id);
      const device = devices.get(id);
      if (device) set(id, assess(device));
    };
    if (held.delete(id)) waiting.set(id, run);
    else timers.set(id, setTimeout(run, delayMs));
  }

  return {
    wrote({ device, orientation }: { device: string; orientation: Orientation }) {
      const shown = read(device, orientation);
      if (shown?.ok) pushed.set(slot(device, orientation), canonical(shown.layout));
      set(device, CHECKING);
      reassess(device);
    },
    relink(frame: Frame): Frame {
      if (frame.type !== 'device.updated' && frame.type !== 'device.added') return frame;
      const id = typeof frame.device === 'string' ? frame.device : '';
      const device = devices.get(id);
      if (!device) return frame;
      const link = JSON.stringify(device.link);
      const was = links.get(id);
      links.set(id, link);
      if (was === link) return frame;
      reassess(id);
      if (was === undefined || sameAssessment(device.control_layout, CHECKING)) return frame;
      const next = { ...device, control_layout: CHECKING };
      devices.set(id, next);
      return { ...frame, data: next };
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
