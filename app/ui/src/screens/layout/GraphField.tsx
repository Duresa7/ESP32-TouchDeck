import { Toggle } from '@base-ui/react/toggle';
import { ToggleGroup } from '@base-ui/react/toggle-group';
import { useDevices } from '../../companion/devices.ts';
import { useIntegrations } from '../../companion/integrations.ts';
import { needsFirmwareField } from '../../lib/device/needs-firmware.ts';
import { graphBinding, graphSources, sourceKey, type GraphSource } from '../../lib/editor/graph.ts';
import type { EditorStore } from '../../lib/editor/store.ts';
import type { Tile } from '../../lib/editor/validate.ts';
import { useOfferedFirmware } from './device.ts';
import { useDeviceShows, useEditorState } from './editor.ts';
import { described, Field, Select, useFieldIds } from './fields.tsx';

function groups(sources: readonly GraphSource[]): { name: string; sources: GraphSource[] }[] {
  const named = new Map<string, GraphSource[]>();
  for (const source of sources) named.set(source.integrationName, [...(named.get(source.integrationName) ?? []), source]);
  return [...named].map(([name, members]) => ({ name, sources: members }));
}

function StreamField({ store, tile, sources }: { store: EditorStore; tile: Tile; sources: readonly GraphSource[] }) {
  const ids = useFieldIds();
  const binding = graphBinding(tile);
  if (!binding) return null;
  const key = sourceKey(binding);
  const known = sources.some((source) => sourceKey(source) === key);
  const hint = 'Updates about once a second.';
  return (
    <Field ids={ids} label="Stream" hint={hint}>
      <Select
        id={ids.control}
        value={key}
        onChange={(event) => {
          const chosen = sources.find((source) => sourceKey(source) === event.target.value);
          if (chosen) store.setGraph(tile.id, chosen);
        }}
        data-testid="graph-stream"
        {...described(ids, hint, null)}
      >
        {!known && <option value={key}>{binding.stream} (unknown)</option>}
        {groups(sources).map((group) => (
          <optgroup key={group.name} label={group.name}>
            {group.sources.map((source) => (
              <option key={sourceKey(source)} value={sourceKey(source)}>
                {source.name}
              </option>
            ))}
          </optgroup>
        ))}
      </Select>
    </Field>
  );
}

/** Whether the Tile shows its label or value, or draws a graph of a numeric Integration stream. */
export function GraphField({ store, tile, device }: { store: EditorStore; tile: Tile; device: string }) {
  const ids = useFieldIds();
  const integrations = useIntegrations((s) => s.integrations);
  const shows = useDeviceShows(store, 'graph');
  const id = useEditorState(store, (s) => s.device?.id);
  const firmware = useDevices((s) => s.devices.find((d) => d.id === id)?.firmware ?? null);
  const offered = useOfferedFirmware({ firmware });
  const sources = graphSources(integrations);
  const graph = tile.render === 'graph';
  const plain = tile.action.type === 'widget' ? 'Value' : 'Label';
  const hint = !shows ? needsFirmwareField({ device, since: offered }) : sources.length === 0 ? 'Set up an Integration to draw a graph.' : null;
  const choose = (value: string) => {
    if (value === 'plain') {
      store.setGraph(tile.id, null);
      return;
    }
    const { action } = tile;
    const current = action.type === 'widget' ? sources.find((s) => s.integration === action.source && s.stream === action.field) : undefined;
    const chosen = current ?? sources[0];
    if (chosen) store.setGraph(tile.id, chosen);
  };
  return (
    <>
      <div className="field">
        <span className="field-label" id={ids.control}>
          Shows
        </span>
        <ToggleGroup
          value={[graph ? 'graph' : 'plain']}
          onValueChange={(value) => {
            if (value[0]) choose(value[0]);
          }}
          aria-labelledby={ids.control}
          aria-describedby={hint ? ids.hint : undefined}
          className="segmented segmented-fill"
        >
          <Toggle value="plain" className="segment" data-testid="tile-shows-plain">
            {plain}
          </Toggle>
          <Toggle value="graph" className="segment" disabled={!graph && (!shows || sources.length === 0)} data-testid="tile-shows-graph">
            Graph
          </Toggle>
        </ToggleGroup>
        {hint && (
          <p className="field-hint" id={ids.hint} data-testid="tile-shows-hint">
            {hint}
          </p>
        )}
      </div>
      {graph && <StreamField store={store} tile={tile} sources={sources} />}
    </>
  );
}
