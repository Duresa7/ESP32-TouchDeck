import { useId, useRef } from 'react';
import { Switch } from '@base-ui/react/switch';
import { Toggle } from '@base-ui/react/toggle';
import { ToggleGroup } from '@base-ui/react/toggle-group';
import { useDevices } from '../../lib/companion/devices.ts';
import { useIntegrations } from '../../lib/companion/integrations.ts';
import { needsFirmwareField } from '../../lib/device/needs-firmware.ts';
import { barFor, graphBinding, graphSources, sourceKey, type GraphSource } from '../../lib/editor/graph.ts';
import type { EditorStore } from '../../lib/editor/store.ts';
import type { Tile } from '../../lib/editor/validate.ts';
import { useOfferedFirmware } from './device.ts';
import { useDeviceShows, useEditorState } from './editor.ts';
import { GifInput } from './GifField.tsx';
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

/** A GIF choice beside Label and Graph, for a Device that plays GIFs. */
export interface ShowsGif {
  on: boolean;
  choose(file: File): void;
  stop(): void;
}

function InputField({ store, tile, device }: { store: EditorStore; tile: Tile; device: string }) {
  const id = useId();
  const integrations = useIntegrations((s) => s.integrations);
  const { action } = tile;
  const settable = action.type === 'widget' && integrations.find((i) => i.id === action.source)?.streams.find((s) => s.id === action.field)?.settable === true;
  if (tile.render !== 'bar' || !settable) return null;
  return (
    <div className="field">
      <div className="switch-row">
        <span className="field-label" id={id}>
          Drag to change
        </span>
        <Switch.Root className="switch" checked={tile.input === true} onCheckedChange={(on) => store.setInput(tile.id, on)} aria-labelledby={id} aria-describedby={`${id}-hint`} data-testid="tile-input">
          <Switch.Thumb className="switch-thumb" />
        </Switch.Root>
      </div>
      <p className="field-hint" id={`${id}-hint`}>
        Slide a finger along the bar on {device} to set it.
      </p>
    </div>
  );
}

function NowPlayingField({ store, device, since }: { store: EditorStore; device: string; since: string | null }) {
  const id = useId();
  const shows = useDeviceShows(store, 'now-playing');
  const hint = shows ? 'The title, artist and cover from Media. Tap its buttons to play, pause or skip.' : needsFirmwareField({ device, since });
  return (
    <div className="field">
      <span className="field-label" id={id}>
        Shows
      </span>
      <p className="field-value" aria-labelledby={id} aria-describedby={`${id}-hint`} data-testid="tile-shows-now-playing">
        Now playing
      </p>
      <p className="field-hint" id={`${id}-hint`} data-testid="tile-shows-hint">
        {hint}
      </p>
    </div>
  );
}

/** Whether the Tile shows its label or value, draws a graph of a numeric Integration stream or its stream's bar, or plays a GIF. */
export function GraphField({ store, tile, device, gif }: { store: EditorStore; tile: Tile; device: string; gif?: ShowsGif }) {
  const ids = useFieldIds();
  const gifFile = useRef<HTMLInputElement>(null);
  const integrations = useIntegrations((s) => s.integrations);
  const shows = useDeviceShows(store, 'graph');
  const showsBar = useDeviceShows(store, 'bar');
  const id = useEditorState(store, (s) => s.device?.id);
  const firmware = useDevices((s) => s.devices.find((d) => d.id === id)?.firmware ?? null);
  const offered = useOfferedFirmware({ firmware });
  if (tile.render === 'now-playing') return <NowPlayingField store={store} device={device} since={offered} />;
  const sources = graphSources(integrations);
  const graph = tile.render === 'graph';
  const { action } = tile;
  const bar = action.type === 'widget' ? barFor(integrations, action) : null;
  const shown = gif?.on ? 'gif' : tile.render === 'bar' ? 'bar' : graph ? 'graph' : 'plain';
  const plain = action.type === 'widget' ? 'Value' : 'Label';
  const needs = needsFirmwareField({ device, since: offered });
  const hint = gif?.on ? null : tile.render === 'bar' ? (showsBar ? null : needs) : !shows ? needs : sources.length === 0 ? 'Set up an Integration to draw a graph.' : null;
  const choose = (value: string) => {
    if (value === 'gif') {
      gifFile.current?.click();
      return;
    }
    if (gif?.on) gif.stop();
    if (value === 'plain') {
      if (tile.render !== undefined) store.setGraph(tile.id, null);
      return;
    }
    if (value === 'bar') {
      if (action.type === 'widget' && bar) store.setLive(tile.id, action, bar);
      return;
    }
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
          value={[shown]}
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
          {(bar !== null || tile.render === 'bar') && (
            <Toggle value="bar" className="segment" disabled={tile.render !== 'bar' && !showsBar} data-testid="tile-shows-bar">
              Bar
            </Toggle>
          )}
          {gif && (
            <Toggle value="gif" className="segment" data-testid="tile-shows-gif">
              GIF
            </Toggle>
          )}
        </ToggleGroup>
        {gif && <GifInput inputRef={gifFile} onFile={gif.choose} testId="tile-shows-gif-file" />}
        {hint && (
          <p className="field-hint" id={ids.hint} data-testid="tile-shows-hint">
            {hint}
          </p>
        )}
      </div>
      {graph && <StreamField store={store} tile={tile} sources={sources} />}
      <InputField store={store} tile={tile} device={device} />
    </>
  );
}
