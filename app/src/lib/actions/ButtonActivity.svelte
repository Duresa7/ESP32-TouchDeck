<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { plural } from '../editor/messages.ts';

  interface LayoutSummary {
    name: string;
    source: string;
    pages: number;
    tiles: number;
  }

  interface ActionLog {
    at_ms: number;
    page: string;
    tile: string;
    kind: string;
    status: string;
    detail: string;
    pid: number | null;
    exit_code: number | null;
  }

  let layout = $state<LayoutSummary | null>(null);
  let log = $state<ActionLog[]>([]);
  let problem = $state<string | null>(null);

  async function loadFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    problem = null;
    try {
      layout = await invoke<LayoutSummary>('set_action_layout', { json: await file.text(), source: file.name });
    } catch (error) {
      problem = String(error);
    }
    input.value = '';
  }

  const time = (ms: number) => new Date(ms).toLocaleTimeString();

  /** What happened to a press, in plain words. */
  function outcome(entry: ActionLog): string {
    switch (entry.status) {
      case 'started':
        return `started${entry.pid === null ? '' : ` (process ${entry.pid})`}`;
      case 'exited':
        return entry.exit_code === 0 ? 'finished' : `finished with code ${entry.exit_code ?? '?'}`;
      case 'denied':
        return 'not run, you said no';
      case 'failed':
        return "didn't work";
      case 'unknown':
        return "ignored, it isn't in the layout Open TouchDeck knows";
      default:
        return `${entry.status}${entry.exit_code === null ? '' : ` with code ${entry.exit_code}`}`;
    }
  }

  onMount(() => {
    invoke<LayoutSummary | null>('action_layout').then((summary) => (layout = summary ?? layout));
    invoke<ActionLog[]>('action_log').then((entries) => (log = [...entries, ...log]));
    const listeners = [
      listen<LayoutSummary>('action-layout', (event) => (layout = event.payload)),
      listen<ActionLog>('action-log', (event) => (log = [...log.slice(-99), event.payload])),
    ];
    return () => listeners.forEach((listener) => listener.then((stop) => stop()));
  });
</script>

<div class="activity" data-testid="actions">
  <div class="source">
    <p data-testid="action-layout">
      {#if layout}
        Buttons come from <strong>“{layout.name}”</strong>
        ({layout.source === 'deck' ? 'read from your deck' : layout.source}), {plural(layout.pages, 'page')}, {plural(layout.tiles, 'button')}.
      {:else}
        Open TouchDeck reads your deck's buttons when it connects.
      {/if}
    </p>
    <label class="button small">
      Use a layout file instead…
      <input type="file" accept=".json,application/json" onchange={loadFile} data-testid="action-layout-file" />
    </label>
  </div>
  {#if problem}
    <p class="problem" data-testid="action-problem">That file couldn't be used: {problem}</p>
  {/if}

  <ol class="entries" data-testid="action-log">
    {#each log as entry, i (i)}
      <li data-testid="action-entry" data-tile={entry.tile} data-status={entry.status} data-exit={entry.exit_code ?? ''} data-pid={entry.pid ?? ''}>
        <span class="when">{time(entry.at_ms)}</span>
        <span class="what"><strong>{entry.tile}</strong> {outcome(entry)}</span>
        {#if entry.detail}<span class="detail" title={entry.detail}>{entry.detail}</span>{/if}
      </li>
    {:else}
      <li class="none">No presses yet. Press a button on your deck that opens an app or runs a command.</li>
    {/each}
  </ol>
</div>

<style>
  .activity {
    display: grid;
    gap: var(--space-3);
  }

  .source {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .button input {
    display: none;
  }

  .entries {
    max-height: 200px;
    margin: 0;
    padding: var(--space-2) var(--space-3);
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--sunken);
    font-size: var(--text-s);
    list-style: none;
  }

  .entries li {
    display: flex;
    gap: var(--space-3);
    padding: 3px 0;
  }

  .when {
    flex: none;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .what {
    flex: none;
  }

  .detail {
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-family: var(--mono);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .none {
    color: var(--muted);
  }
</style>
