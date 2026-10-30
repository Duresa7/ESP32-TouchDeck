import type { IntegrationSummary, PresetTile } from '../../companion/api.ts';
import { parseAction, presetLabel } from '../integrations/model.ts';
import type { GraphBinding, Tile } from './validate.ts';

/** A numeric Integration stream a graph Tile can draw, named as its Preset tile when it has one. */
export interface GraphSource extends GraphBinding {
  integrationName: string;
  name: string;
  unit: string | null;
}

/** Every numeric stream of every Integration, in the Companion's order. */
export function graphSources(integrations: readonly IntegrationSummary[]): GraphSource[] {
  return integrations.flatMap((integration) =>
    integration.streams
      .filter((stream) => stream.kind === 'numeric')
      .map((stream) => {
        const preset = integration.presets.find((p) => {
          const action = parseAction(p.action);
          return action?.type === 'widget' && action.source === integration.id && action.field === stream.id;
        });
        return {
          integration: integration.id,
          stream: stream.id,
          integrationName: integration.name,
          name: preset ? presetLabel(preset.name) : stream.id,
          unit: stream.unit,
        };
      }),
  );
}

/** The stream a graph Tile draws, or null for any other Tile. */
export function graphBinding(tile: Tile): GraphBinding | null {
  if (tile.render !== 'graph' || tile.action.type !== 'widget') return null;
  return { integration: tile.action.source, stream: tile.action.field };
}

export const sourceKey = (binding: GraphBinding) => `${binding.integration}/${binding.stream}`;

/** The stream a Preset tile draws as a graph, when its Integration offers it as one and the stream is numeric. */
export function presetGraph(preset: PresetTile, integration: Pick<IntegrationSummary, 'id' | 'streams'>): GraphBinding | null {
  const action = parseAction(preset.action);
  if (preset.render !== 'graph' || action?.type !== 'widget' || action.source !== integration.id) return null;
  const stream = integration.streams.find((s) => s.id === action.field);
  return stream?.kind === 'numeric' ? { integration: integration.id, stream: stream.id } : null;
}
