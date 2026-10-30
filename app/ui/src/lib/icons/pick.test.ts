import { afterEach, describe, expect, it, vi } from 'vitest';
import { createClient } from '../../companion/client.ts';
import { createFakeCompanion } from '../../companion/fake.ts';
import { RECORDED_HITS } from '../../companion/fake-catalog.data.ts';
import { createJobsStore, waitForJob } from '../../companion/jobs.ts';
import { windowSession } from '../../companion/session.ts';
import { decodeTdi1 } from './tdi1.ts';
import { catalogProblem, renderCatalog, type PickDeps } from './pick.ts';

function setup(renderMs = 5) {
  const fake = createFakeCompanion({ assets: { renderMs } });
  vi.stubGlobal('fetch', fake.fetch);
  const companion = createClient('http://127.0.0.1:51234', windowSession(fake.bridge));
  const jobs = createJobsStore();
  const deps: PickDeps = { companion, wait: (job, signal) => waitForJob<'assets.render'>(jobs, companion, job, { pollMs: 10, signal }) };
  return { fake, companion, deps };
}

function recorded(id: string) {
  const hit = RECORDED_HITS.find((h) => h.source === 'simple-icons' && h.id === id);
  if (!hit) throw new Error(`no recorded Simple Icons hit ${id}`);
  return hit;
}

const discord = recorded('discord');

describe('picking a logo', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('creates the Asset through assets.create with the catalog source', async () => {
    const { fake, companion, deps } = setup();
    const ref = await renderCatalog(deps, discord, new AbortController().signal);
    expect(fake.assets.created).toEqual([{ kind: 'catalog', catalog: 'simple-icons', id: 'discord', variant: 'base', padding: 0, background: null, tint: [0x58, 0x65, 0xf2] }]);
    expect(ref.id).toMatch(/^[0-9a-f]{64}$/);
    const file = await companion.assets.file({ asset: ref.id });
    expect(decodeTdi1(new Uint8Array(await file.arrayBuffer()))).not.toBeNull();
  });

  it('reports a failed render in the copy guide’s words', async () => {
    const { fake, deps } = setup();
    fake.assets.failRender('discord');
    const error = await renderCatalog(deps, discord, new AbortController().signal).catch((e: unknown) => e);
    expect(catalogProblem(error, 'Discord')).toBe("Couldn't get Discord. Check the internet connection, then retry.");
  });

  it('cancels the render Job when the picker closes mid-render', async () => {
    const { fake, deps } = setup(10_000);
    const controller = new AbortController();
    const picking = renderCatalog(deps, discord, controller.signal);
    await vi.waitFor(() => expect(fake.assets.jobs()).toHaveLength(1));
    controller.abort();
    await expect(picking).rejects.toBeDefined();
    await vi.waitFor(() => expect(fake.assets.jobs()[0].state).toBe('cancelled'));
  });
});
