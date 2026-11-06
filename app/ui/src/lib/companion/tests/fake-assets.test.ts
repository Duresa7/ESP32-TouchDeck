import { expect, it } from 'vitest';
import { createFakeAssets } from '../fake.ts';

const source = { kind: 'gif', upload: 'upl_0000000000000001', device: 'mock0001', target: { width: 128, height: 128, background: [20, 30, 40] } };
const request = (path: string, body: unknown) => new Request(`http://example.invalid${path}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });


it('returns a GIF Asset from the fake render Job', async () => {
  const fake = createFakeAssets({ publish() {}, renderMs: 1 });
  const uploaded = await fake.handle(new Request('http://example.invalid/uploads', { method: 'POST', headers: { 'Content-Type': 'image/gif' }, body: 'GIF89a' }), '/uploads');
  expect(uploaded?.status).toBe(200);
  expect((await fake.handle(request('/assets', { source, fit: 'contain' }), '/assets'))?.status).toBe(202);
  await expect.poll(() => fake.jobs()[0].state).toBe('succeeded');
  const job = fake.jobs()[0];
  expect(job.state).toBe('succeeded');
  expect(job.result).toMatchObject({ kind: 'gif', width: 128, height: 128 });
});

it('rejects an invalid GIF fit before starting a Job', async () => {
  const fake = createFakeAssets({ publish() {} });
  expect((await fake.handle(request('/assets', { source, fit: 'stretch' }), '/assets'))?.status).toBe(400);
  expect(fake.jobs()).toHaveLength(0);
});

it('rejects a GIF target outside 1 to 4096 before starting a Job', async () => {
  const fake = createFakeAssets({ publish() {} });
  for (const size of [{ width: 0, height: 128 }, { width: 128, height: 4097 }]) {
    const response = await fake.handle(request('/assets', { source: { ...source, target: { ...source.target, ...size } } }), '/assets');
    expect(response?.status).toBe(400);
    expect(await response?.json()).toMatchObject({ error: { code: 'request.invalid' } });
  }
  expect(fake.jobs()).toHaveLength(0);
});
