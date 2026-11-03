import type { DeviceSummary as GeneratedDeviceSummary, Orientation, SyncData as GeneratedSyncData } from './api.gen.ts';

export * from './api.gen.ts';

export type ControlLayoutUnavailable = 'offline' | 'checking' | 'unsupported' | 'read-failed';
export type ControlLayout =
  | { state: 'unavailable'; reason: ControlLayoutUnavailable }
  | { state: 'no-pushed-copy'; orientation: Orientation; has_controls: boolean }
  | { state: 'matches'; orientation: Orientation; has_controls: boolean }
  | { state: 'differs'; orientation: Orientation; has_controls: boolean };
export type DeviceSummary = GeneratedDeviceSummary & { control_layout: ControlLayout };
export type DeviceAdded = DeviceSummary;
export type DeviceUpdated = DeviceSummary;
export type SyncData = Omit<GeneratedSyncData, 'devices'> & { devices: DeviceSummary[] };
