import { expect, test } from 'vitest';
import { createFakeCompanion } from './fake.ts';
import { fakeThisPc } from './fake-this-pc.ts';

test('This PC is discoverable with ten ordered Presets through the fake API', async () => {
  const fake = createFakeCompanion();
  const token = await fake.bridge.session();
  const headers = { Authorization: `Bearer ${token}` };
  const list = await fake.fetch('http://127.0.0.1/api/v1/integrations', { headers });
  const detail = await fake.fetch('http://127.0.0.1/api/v1/integrations/this_pc', { headers });
  const view = fakeThisPc();
  expect(list.status).toBe(200);
  expect(detail.status).toBe(200);
  expect(await detail.json()).toEqual(view);
  expect(await list.json()).toContainEqual(view.summary);
  expect(view.config).toEqual({ enabled: true });
  expect(view.secrets).toEqual({});
  expect(view.summary.presets.map((p) => p.name)).toEqual([
    'CPU usage', 'RAM usage', 'RAM used', 'RAM total', 'GPU usage', 'Disk activity',
    'Disk read', 'Disk write', 'Network receive', 'Network send',
  ]);
  expect(view.summary.streams).toHaveLength(10);
});
