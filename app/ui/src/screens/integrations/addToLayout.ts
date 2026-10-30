import { useCallback } from 'react';
import { toast } from 'sonner';
import type { DeviceSummary, PresetTile } from '../../companion/api.ts';
import { useCompanion } from '../../companion/connection.ts';
import { useDevices } from '../../companion/devices.ts';
import { useIntegrations } from '../../companion/integrations.ts';
import { showsKind } from '../../lib/device/needs-firmware.ts';
import { presetGraph } from '../../lib/editor/graph.ts';
import { useUi } from '../../state/ui.ts';

/** The Device the Layout screen edits: the one Home opened, or the first named one. */
export function currentDevice(devices: readonly DeviceSummary[], deviceId: string | null): DeviceSummary | undefined {
  return deviceId ? devices.find((d) => d.id === deviceId) : devices.find((d) => d.name !== null);
}

/**
 * Add to Layout: puts a Preset tile on the current Device's draft through the editor store, and says how it went. A
 * Preset its Integration offers as a graph becomes a graph Tile when the Device draws them.
 */
export function useAddToLayout() {
  const companion = useCompanion();
  const deviceId = useUi((s) => s.deviceId);
  const device = useDevices((s) => currentDevice(s.devices, deviceId));
  const integrations = useIntegrations((s) => s.integrations);
  return useCallback(
    async (preset: PresetTile) => {
      if (!device?.name) {
        toast.error('Plug in a Device to add Tiles.');
        return;
      }
      const { editorFor } = await import('../layout/editor.ts');
      const store = editorFor(companion);
      store.open({ id: device.id, name: device.name }, device.orientation === 'portrait' ? 'portrait' : 'landscape');
      const integration = integrations.find((i) => i.presets.includes(preset));
      const graph = integration && showsKind(device, 'graph') ? presetGraph(preset, integration) : null;
      const status = await store.addPreset(preset, graph);
      if (status?.kind === 'error') toast.error(status.text);
      else if (status) toast.success(status.text);
    },
    [companion, device, integrations],
  );
}
