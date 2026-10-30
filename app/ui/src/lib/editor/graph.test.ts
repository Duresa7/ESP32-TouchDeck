import assert from 'node:assert/strict';
import { test } from 'vitest';
import type { IntegrationSummary } from '../../companion/api.ts';
import { fakeThisPc } from '../../companion/fake-this-pc.ts';
import { graphBinding, graphSources, presetGraph, sourceKey } from './graph.ts';
import { graphTile, newTile } from './validate.ts';

const thisPc = fakeThisPc().summary;

const hub: IntegrationSummary = {
  ...thisPc,
  id: 'hub',
  name: 'Hub',
  streams: [
    { id: 'title', kind: 'text', unit: null },
    { id: 'five_hour_pct', kind: 'numeric', unit: '%' },
    { id: 'requests', kind: 'numeric', unit: null },
  ],
  presets: [{ id: 'five_hour_pct', name: 'five hour pct', action: { type: 'widget', source: 'hub', field: 'five_hour_pct', account: '' } }],
};

test('the stream picker lists every numeric stream of every Integration, in order, named by its Preset tile', () => {
  const sources = graphSources([thisPc, hub]);
  assert.equal(sources.length, 12);
  assert.deepEqual(sources.slice(0, 2), [
    { integration: 'this_pc', stream: 'cpu_pct', integrationName: 'This PC', name: 'CPU usage', unit: '%' },
    { integration: 'this_pc', stream: 'ram_pct', integrationName: 'This PC', name: 'RAM usage', unit: '%' },
  ]);
  assert.deepEqual(
    sources.slice(10).map((s) => [s.integrationName, s.name, s.unit]),
    [
      ['Hub', 'Five hour pct', '%'],
      ['Hub', 'requests', null],
    ],
  );
  assert.equal(new Set(sources.map(sourceKey)).size, sources.length);
});

test('with no numeric streams there is nothing to pick', () => {
  assert.deepEqual(graphSources([]), []);
  assert.deepEqual(graphSources([{ ...hub, streams: hub.streams.filter((s) => s.kind !== 'numeric') }]), []);
});

test('a graph Tile reads the Integration stream its widget action names', () => {
  const tile = graphTile(newTile('t', 0, 0), { integration: 'this_pc', stream: 'gpu_pct' });
  assert.deepEqual(graphBinding(tile), { integration: 'this_pc', stream: 'gpu_pct' });
  assert.equal(graphBinding(newTile('u', 0, 1)), null);
  assert.equal(graphBinding({ ...newTile('w', 0, 2), action: { type: 'widget', source: 'this_pc', field: 'cpu_pct', account: '' } }), null);
});

test('a Preset becomes a graph only when its Integration offers it as one and its stream is numeric', () => {
  const cpu = thisPc.presets[0];
  assert.deepEqual(presetGraph(cpu, thisPc), { integration: 'this_pc', stream: 'cpu_pct' });
  assert.equal(presetGraph(hub.presets[0], hub), null);
  assert.equal(presetGraph({ ...cpu, render: 'graph' }, hub), null);
  const text = { ...hub, streams: [{ id: 'cpu_pct', kind: 'text', unit: null }], id: 'this_pc' };
  assert.equal(presetGraph(cpu, text), null);
  assert.equal(presetGraph({ ...cpu, action: { type: 'none' } }, thisPc), null);
});
