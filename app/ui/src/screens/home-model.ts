import type { DeviceSummary } from '../companion/api.ts';
import { parseLayout } from '../lib/editor/validate.ts';

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

// Phase A envelope: the merged API leaves LayoutSummary as JSON. Keep this
// adapter separate until #74 provides the typed current Page and Layout route.
export function previewOf(value: DeviceSummary['layout']) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const parsed = parseLayout(JSON.stringify(value.document));
  if (!parsed.ok) return null;
  const index = parsed.layout.pages.findIndex((page) => page.id === value.current_page);
  if (index < 0) return null;
  return { layout: parsed.layout, index };
}

export function lastSeen(device: DeviceSummary) {
  if (device.link.state !== 'lost' || device.link.at_ms <= 0) return null;
  const date = new Date(device.link.at_ms);
  return Number.isNaN(date.getTime()) ? null : `Last seen ${date.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}`;
}
