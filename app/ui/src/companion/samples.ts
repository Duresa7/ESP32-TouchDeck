import type { DeviceSummary, LinkState } from './api.ts';

// A DeviceSummary as the Companion reports it, for the fake Companion and tests.
export function sampleDevice(id: string, name: string | null, link: LinkState = { state: 'offline' }): DeviceSummary {
  return {
    id,
    rev: 1,
    name,
    setup: name === null ? 'needs-setup' : 'ready',
    link,
    firmware: { version: '1.4.0', protocol: 2, build: 'fake', recovery: false, update_available: null },
    capabilities: null,
    orientation: 'landscape',
    layout: null,
    key_mirror: { enabled: true, reason: null },
    stable_id: true,
  };
}

const connected = (port: string): LinkState => ({ state: 'connected', port, since_ms: 0, protocol: 2 });

// The Devices the demo mode starts with: two connected, one unplugged.
export const demoDevices: DeviceSummary[] = [
  sampleDevice('mock0001', 'Desk', connected('COM3')),
  sampleDevice('mock0002', 'Studio', connected('COM4')),
  sampleDevice('mock0003', 'Stream deck by the window', { state: 'lost', reason: 'unplugged', detail: '', at_ms: Date.UTC(2026, 9, 27, 21, 40) }),
];

// Phase A data only. The typed Layout endpoint/current Page arrives with #74.
export function demoLayout(currentPage = 'media') {
  const labels = ['Previous', 'Play/Pause', 'Next', 'Mute', 'Volume', 'Music', 'Work', 'Break'];
  const pages = ['media', 'work'].map((id) => ({
    id, title: id === 'media' ? 'Media' : 'Work', kind: 'grid', rows: 2, cols: 4,
    tiles: labels.map((label, i) => ({ id: `${id}${i}`, label: id === 'media' ? label : ['Copy', 'Paste', 'Undo', 'Save', 'Find', 'Home', 'End', 'Escape'][i], row: Math.floor(i / 4), col: i % 4, rowSpan: 1, colSpan: 1, icon: null, action: { type: 'none' } })),
  }));
  return { document: { schema: 1, name: 'Demo Layout', orientation: 'landscape', pages }, current_page: currentPage };
}

for (const device of demoDevices) device.layout = demoLayout();
if (demoDevices[1].firmware) demoDevices[1].firmware.update_available = '1.5.0';

export function demoScenario(count: number) {
  return Array.from({ length: count }, (_, i) => ({ ...structuredClone(demoDevices[i % 3]), id: `mock${String(i + 1).padStart(4, '0')}` }));
}
