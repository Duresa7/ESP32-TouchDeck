import { useCallback } from 'react';
import { toast } from 'sonner';
import type { DeviceSummary, PresetTile } from '../../companion/api.ts';
import { useCompanion } from '../../companion/connection.ts';
import { useDevices } from '../../companion/devices.ts';
import { useUi } from '../../state/ui.ts';

/** The Device the Layout screen edits: the one Home opened, or the first named one. */
export function currentDevice(devices: readonly DeviceSummary[], deviceId: string | null): DeviceSummary | undefined {
  return deviceId ? devices.find((d) => d.id === deviceId) : devices.find((d) => d.name !== null);
}

/** Add to Layout: puts a Preset tile on the current Device's draft through the editor store, and says how it went. */
export function useAddToLayout() {
  const companion = useCompanion();
  const deviceId = useUi((s) => s.deviceId);
  const device = useDevices((s) => currentDevice(s.devices, deviceId));
  return useCallback(
    async (preset: PresetTile) => {
      if (!device?.name) {
        toast.error('Plug in a Device to add Tiles.');
        return;
      }
      const { editorFor } = await import('../layout/editor.ts');
      const store = editorFor(companion);
      store.open({ id: device.id, name: device.name }, device.orientation === 'portrait' ? 'portrait' : 'landscape');
      const status = await store.addPreset(preset);
      if (status?.kind === 'error') toast.error(status.text);
      else if (status) toast.success(status.text);
    },
    [companion, device],
  );
}
