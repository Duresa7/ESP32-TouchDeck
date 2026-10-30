import type { DeviceSummary, Layout, LinkState, Page } from './api.ts';

export function sampleDevice(id: string, name: string | null, link: LinkState = { state: 'offline' }): DeviceSummary {
  return {
    id,
    rev: 1,
    name,
    setup: name === null ? 'needs-setup' : 'ready',
    link,
    firmware: { version: '1.5.0', protocol: 2, build: 'fake', recovery: false, update_available: null },
    diagnostics: { product: 'touchdeck', running_from: 'ota_0', sinks: ['log', 'link'], current_layout: 'builtin' },
    capabilities: null,
    orientation: 'landscape',
    layout: null,
    key_mirror: { enabled: true, reason: null },
    stable_id: true,
  };
}

const connected = (port: string): LinkState => ({ state: 'connected', port, since_ms: 0, protocol: 2 });

export const demoDevices: DeviceSummary[] = [
  sampleDevice('mock0001', 'Desk', connected('COM3')),
  sampleDevice('mock0002', 'Studio', connected('COM4')),
  sampleDevice('mock0003', 'Stream deck by the window', connected('COM5')),
];
if (demoDevices[1].firmware) demoDevices[1].firmware = { ...demoDevices[1].firmware, version: '1.4.0', update_available: '1.5.0' };

const TILES: Record<string, string[]> = {
  media: ['Previous', 'Play/Pause', 'Next', 'Mute', 'Volume', 'Music', 'Work', 'Break'],
  work: ['Copy', 'Paste', 'Undo', 'Save', 'Find', 'Home', 'End', 'Escape'],
};

export function demoLayout(order = ['media', 'work']): Layout {
  const pages: Page[] = order.map((id) => ({
    kind: 'grid', id, title: id === 'media' ? 'Media' : 'Work', rows: 2, cols: 4,
    tiles: TILES[id].map((label, i) => ({ id: `${id}${i}`, label, row: Math.floor(i / 4), col: i % 4, rowSpan: 1, colSpan: 1, icon: null, action: { type: 'none' } })),
  }));
  return { schema: 1, name: 'Demo Layout', orientation: 'landscape', pages };
}

export function demoScenario(count: number) {
  return Array.from({ length: count }, (_, i) => ({ ...structuredClone(demoDevices[i % 3]), id: `mock${String(i + 1).padStart(4, '0')}` }));
}
