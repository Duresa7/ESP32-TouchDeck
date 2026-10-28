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
  sampleDevice('mock0003', 'Stream deck by the window', { state: 'lost', reason: 'unplugged', detail: '', at_ms: 0 }),
];
