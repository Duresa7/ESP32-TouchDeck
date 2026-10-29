import type { DeviceSummary, Layout, Orientation } from '../companion/api.ts';

export function statusOf(state: string) {
  switch (state) {
    case 'connected': return { words: 'Connected', live: true };
    case 'connecting': return { words: 'Connecting…', live: false };
    case 'busy': return { words: 'In use by another program', live: false };
    case 'updating': return { words: 'Updating…', live: false };
    case 'restarting': return { words: 'Restarting…', live: false };
    default: return { words: 'Not connected', live: false };
  }
}

export const deviceWidth = (count: number) => count === 1 ? 560 : 312;
export const firmwareUpdate = (device: DeviceSummary) => Boolean(device.firmware?.update_available);
export const orientationOf = (device: DeviceSummary): Orientation => device.orientation === 'portrait' ? 'portrait' : 'landscape';

export interface Preview {
  layout: Layout;
  index: number;
}

// The contract has no field for the Page a Device shows, so the miniature shows Page 1.
export function previewOf(layout: Layout | null | undefined): Preview | null {
  return layout?.pages.length ? { layout, index: 0 } : null;
}

export function lastSeen(device: DeviceSummary) {
  if (device.link.state !== 'lost' || device.link.at_ms <= 0) return null;
  const date = new Date(device.link.at_ms);
  return Number.isNaN(date.getTime()) ? null : `Last seen ${date.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}`;
}
