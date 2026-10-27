import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { Layout, Orientation } from '../../src/lib/editor/validate.ts';
import type { SourcesView, Status } from '../../src/lib/sources/api.ts';
import { connectFirstDeck } from '../deck.ts';
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
          id: `feed_${orientation === 'landscape' ? 'land' : 'port'}${revision}`, label: 'Context',
          row: 0, col: 1, rowSpan: 1, colSpan: 1, icon: null,
          action: { type: 'widget', source: 'claude-status', field: 'context_pct', account: '' },
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
  let statusPath = '';
  const writeStatus = (percent: number) => writeFileSync(statusPath, JSON.stringify({
    session_id: 'orientation-fixture', context_window: { used_percentage: percent },
  }));

  before(async () => {
    note('port', await connectFirstDeck());
    const view = await invoke<SourcesView>('sources_view');
    mkdirSync(view.claude_dir, { recursive: true });
    statusPath = join(view.claude_dir, 'orientation-fixture.json');
    writeStatus(21);
    await invoke('sources_save', { config: { ...view.config, claude_status: true, cliproxy: false, layout_path: null } });
  });

  it('keeps displayed widget bindings after pushing an inactive slot, without an action press', async () => {
    await push(fixture('landscape'));
    await until(async () => (await mockSets()).find((set) => set.tile === 'feed_land' && set.value === '21%'), 5_000, 'the initial landscape feed');
    const before = (await mockSets()).length;
    const report = await push(fixture('portrait'));
    expect(report.state).toMatchObject({ orientation: 'landscape', name: 'landscape' });
    writeStatus(37);
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
    writeStatus(58);
    const status = await sourceStatus();
    saveJson('saved-reconnect.json', { program, status, mock: await invoke('e2e_mock_record') });
    expect(program).toBe('touchdeck-fixture-landscape');
    expect(status.layout.name).toBe('landscape');
    await until(async () => (await mockSets()).find((set) => set.tile === 'feed_land' && set.value === '58%'), 5_000, 'the reconnected landscape feed');
    await $('[data-testid="nav-actions"]').click();
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
    writeStatus(73);
    await until(async () => (await mockSets()).slice(before).find((set) => set.tile === 'feed_port_v2' && set.value === '73%'), 5_000, 'the replacement portrait feed');
    expect(await approvalProgram()).toBe('touchdeck-fixture-portrait_v2');
    saveJson('active-push.json', { report, status: await sourceStatus(), sets: (await mockSets()).slice(before) });
    const input = await invoke<string[] | null>('e2e_input_log');
    note('input_events', input);
    expect(input ?? []).toEqual([]);
  });
});
