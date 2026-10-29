import type { DeviceSummary, JsonValue } from '../../companion/api.ts';

export interface Fact {
  key: 'product' | 'proto' | 'elf_sha256' | 'version' | 'running' | 'sinks' | 'recovery' | 'layout';
  label: string;
  value: string | null;
}

export const nameOf = (device: DeviceSummary) => device.name ?? 'Unnamed Device';

const yesNo = (value: boolean) => (value ? 'Yes' : 'No');

function layoutName(layout: JsonValue | null): string | null {
  if (typeof layout !== 'object' || layout === null || Array.isArray(layout)) return null;
  return typeof layout.name === 'string' ? layout.name : null;
}

/** `DeviceSummary` doesn't carry the product, the running slot or the key sinks yet, so those read as unknown. */
export function factsOf(device: DeviceSummary): Fact[] {
  const firmware = device.firmware;
  return [
    { key: 'product', label: 'Product', value: null },
    { key: 'proto', label: 'Protocol', value: firmware && String(firmware.protocol) },
    { key: 'elf_sha256', label: 'Firmware build', value: firmware && firmware.build },
    { key: 'version', label: 'Firmware version', value: firmware && firmware.version },
    { key: 'running', label: 'Running from', value: null },
    { key: 'sinks', label: 'Sends keys through', value: null },
    { key: 'recovery', label: 'Safe mode', value: firmware && yesNo(firmware.recovery) },
    { key: 'layout', label: 'Current Layout', value: layoutName(device.layout) },
  ];
}

export function connectionText({ device, demo }: { device: DeviceSummary; demo: boolean }): string {
  const name = nameOf(device);
  const { link } = device;
  switch (link.state) {
    case 'connected':
      return demo ? `${name} is the demo Device.` : `${name} is on ${link.port}.`;
    case 'connecting':
      return 'Connecting…';
    case 'busy':
      return `Another program is using ${name}. Close it, and ${name} reconnects on its own.`;
    case 'unresponsive':
      return `${name} didn't answer. Unplug it and plug it back in.`;
    case 'lost':
      return `Lost ${name}. Check the USB cable. It reconnects on its own.`;
    case 'updating':
      return `${name} is updating. Wait for it to finish.`;
    case 'offline':
    case 'disconnected':
      return `${name} isn't connected.`;
    default: {
      const _exhaustive: never = link;
      return _exhaustive;
    }
  }
}

export function lastError(device: DeviceSummary): string | null {
  const { link } = device;
  return (link.state === 'lost' || link.state === 'busy') && link.detail ? link.detail : null;
}
