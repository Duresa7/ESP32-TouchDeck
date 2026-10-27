import type { Layout, Orientation } from '../../src/lib/editor/validate.ts';
import type { SourcesView, Status } from '../../src/lib/sources/api.ts';
import { connectFirstDeck, openSettings } from '../deck.ts';
import { DEFAULT_KEY, start, type FixtureServer } from '../fixtures/cliproxy-server.mjs';
import { note, shot } from '../report.ts';
import { invoke, mockSets, saveJson, until } from '../sources.ts';

function fixture(orientation: Orientation, revision = ''): Layout {
  return {
    schema: 1,
    name: `${orientation}${revision}`,
    orientation,
    pages: [{
      id: 'shared', title: 'Bindings', kind: 'grid', rows: 1, cols: 2,
      tiles: [
        {
          id: 'action', label: 'Approval only', row: 0, col: 0, rowSpan: 1, colSpan: 1, icon: null,
          action: { type: 'command', program: `touchdeck-fixture-${orientation}${revision}`, args: [], cwd: '', shell: false },
        },
        {
          id: `feed_${orientation === 'landscape' ? 'land' : 'port'}${revision}`, label: '5h usage',
          row: 0, col: 1, rowSpan: 1, colSpan: 1, icon: null,
          action: { type: 'widget', source: 'cliproxyapi', field: 'five_hour_pct', account: 'codex:alice@example.com' },
        },
      ],
    }],
  };
}

const push = (layout: Layout) => invoke<{ state: { orientation: Orientation; name: string }; icons: { sent: string[] } }>('push_layout', { layout: JSON.stringify(layout) });
const sourceStatus = () => invoke<Status>('sources_status');

async function approvalProgram() {
  await invoke('e2e_emit', { events: [{ m: 'press', page: 'shared', tile: 'action', kind: 'command' }] });
  const request = await until(async () => {
    const pending = await invoke<{ id: number; program: string }[]>('pending_approvals');
    return pending[0];
  }, 5_000, 'the fixture command approval');
  // Never approve or execute the fixture. Matching IDs must select the displayed command.
  await $('[data-testid="approval-deny"]').waitForDisplayed({ timeout: 3_000 });
  await $('[data-testid="approval-deny"]').click();
  return request.program;
}

describe('bindings follow the displayed orientation', () => {
  let server: FixtureServer;
  let lastRefresh = 0;
  async function writeUsage(percent: number) {
    const wait = lastRefresh + 15_100 - Date.now();
    if (wait > 0) await browser.pause(wait);
    server.setFiveHourPercent(percent);
    lastRefresh = Date.now();
    await invoke('sources_refresh', { source: 'cliproxyapi' });
    await until(async () => (await sourceStatus()).proxy.accounts.some((account) =>
      account.key === 'codex:alice@example.com' && account.five_hour?.pct === percent),
    5_000, 'the new source usage reading');
  }

  before(async () => {
    server = await start();
    server.setFiveHourPercent(21);
    note('port', await connectFirstDeck());
    const view = await invoke<SourcesView>('sources_view');
    await invoke('sources_set_key', { key: DEFAULT_KEY });
    await invoke('sources_save', { config: { ...view.config, cliproxy: true, cliproxy_url: server.url, layout_path: null } });
    await until(async () => (await sourceStatus()).proxy.ok === true, 5_000, 'the initial hub poll');
  });

  after(async () => { await server.stop(); });

  it('replaces previous actions and feeds when reconnecting to built-in pages', async () => {
    await openSettings('help');
    const previous = fixture('landscape');
    for (const page of previous.pages) {
      if (page.kind !== 'grid') continue;
      page.id = 'fkeys';
      const action = page.tiles.find((tile) => tile.id === 'action');
      if (action) action.id = 'f1';
    }
    await push(previous);
    await until(async () => (await sourceStatus()).layout.widgets === 1, 5_000, 'the previous widget binding');
    // No portrait slot exists yet, so rebooting into it displays the firmware builtin.
    await invoke('settings_put', { settings: { idle_timeout_s: 600, orientation: 'portrait' } });
    await invoke('reboot_deck');
    await invoke('disconnect');
    await invoke('connect', { port: 'mock' });
    await until(async () => (await invoke<{ name: string } | null>('action_layout'))?.name === 'Built-in', 5_000, 'the built-in action-layout event');
    await until(async () => (await sourceStatus()).layout.widgets === 0, 5_000, 'cleared widget bindings');
    const before = (await mockSets()).length;
    await writeUsage(89);
    await browser.pause(2_100);
    expect((await mockSets()).slice(before).some((set) => set.m === 'widget.set')).toBe(false);
    saveJson('builtin-reconnect.json', { layout: await invoke('action_layout'), status: await sourceStatus(), sets: (await mockSets()).slice(before) });
    await browser.waitUntil(async () => (await $('[data-testid="action-layout"]').getText()).includes('Built-in'), { timeout: 3_000 });
    await shot('builtin-bindings');
    await invoke('settings_put', { settings: { idle_timeout_s: 600, orientation: 'landscape' } });
    await invoke('reboot_deck');
    await invoke('disconnect');
    await invoke('connect', { port: 'mock' });
    await until(async () => (await invoke<{ name: string } | null>('action_layout'))?.name === 'landscape', 5_000, 'the restored landscape binding');
    await writeUsage(21);
  });

  it('keeps displayed widget bindings after pushing an inactive slot, without an action press', async () => {
    await push(fixture('landscape'));
    await until(async () => (await mockSets()).find((set) => set.tile === 'feed_land' && set.value === '21%'), 5_000, 'the initial landscape feed');
    const before = (await mockSets()).length;
    const report = await push(fixture('portrait'));
    expect(report.state).toMatchObject({ orientation: 'landscape', name: 'landscape' });
    await writeUsage(37);
    let active = false;
    try {
      await until(async () => (await mockSets()).slice(before).find((set) => set.tile === 'feed_land' && set.value === '37%'), 5_000, 'the displayed landscape update after an inactive push');
      active = true;
    } finally {
      saveJson('inactive-push.json', { report, active, status: await sourceStatus(), sets: (await mockSets()).slice(before) });
    }
    expect((await mockSets()).slice(before).some((set) => set.tile === 'feed_port')).toBe(false);
    expect(await approvalProgram()).toBe('touchdeck-fixture-landscape');
  });

  it('reconnects to displayed actions and widgets after saving another orientation without rebooting', async () => {
    await push(fixture('landscape'));
    expect(await approvalProgram()).toBe('touchdeck-fixture-landscape');
    await invoke('set_action_layout', { json: JSON.stringify(fixture('landscape')), source: 'await-reconnect' });
    await invoke('settings_put', { settings: { idle_timeout_s: 600, orientation: 'portrait' } });
    await invoke('disconnect');
    await invoke('connect', { port: 'mock' });
    // Both layouts have the same page/action IDs but different harmless commands.
    await until(async () => (await invoke<{ source: string } | null>('action_layout'))?.source === 'deck', 5_000, 'the reconnect layout pull');
    const program = await approvalProgram();
    await writeUsage(58);
    const status = await sourceStatus();
    saveJson('saved-reconnect.json', { program, status, mock: await invoke('e2e_mock_record') });
    expect(program).toBe('touchdeck-fixture-landscape');
    expect(status.layout.name).toBe('landscape');
    await until(async () => (await mockSets()).find((set) => set.tile === 'feed_land' && set.value === '58%'), 5_000, 'the reconnected landscape feed');
    await openSettings('help');
    await shot('displayed-actions');
  });

  it('switches bindings after reboot and updates them when the displayed slot is pushed', async () => {
    await invoke('set_action_layout', { json: JSON.stringify(fixture('landscape')), source: 'await-reconnect' });
    await invoke('reboot_deck');
    await invoke('disconnect');
    await invoke('connect', { port: 'mock' });
    await until(async () => (await invoke<{ source: string } | null>('action_layout'))?.source === 'deck', 5_000, 'the layout pull after reboot');
    expect((await sourceStatus()).layout.name).toBe('portrait');
    expect(await approvalProgram()).toBe('touchdeck-fixture-portrait');
    const before = (await mockSets()).length;
    const report = await push(fixture('portrait', '_v2'));
    expect(report.state).toMatchObject({ orientation: 'portrait', name: 'portrait_v2' });
    await writeUsage(73);
    await until(async () => (await mockSets()).slice(before).find((set) => set.tile === 'feed_port_v2' && set.value === '73%'), 5_000, 'the replacement portrait feed');
    expect(await approvalProgram()).toBe('touchdeck-fixture-portrait_v2');
    saveJson('active-push.json', { report, status: await sourceStatus(), sets: (await mockSets()).slice(before) });
    const input = await invoke<string[] | null>('e2e_input_log');
    note('input_events', input);
    expect(input ?? []).toEqual([]);
  });
});
