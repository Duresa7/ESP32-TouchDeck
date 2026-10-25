<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import KeyMonitor from '../lib/actions/KeyMonitor.svelte';

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

  interface MirrorInfo {
    enabled: boolean;
    reason: string;
  }

  let { online }: { online: boolean } = $props();

  let layout = $state<LayoutSummary | null>(null);
  let log = $state<ActionLog[]>([]);
  let mirror = $state<MirrorInfo | null>(null);
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

  function time(ms: number) {
    return new Date(ms).toLocaleTimeString();
  }

  $effect(() => {
    if (online) invoke<MirrorInfo>('mirror_state').then((info) => (mirror = info));
  });

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

<section class="panel actions" data-testid="actions">
  <div class="panel-head">
    <h2>Actions</h2>
    <span class="hint" data-testid="mirror-state" data-enabled={mirror ? String(mirror.enabled) : null}>
      {online ? (mirror?.reason ?? '') : 'Connect a deck to mirror its keys'}
    </span>
  </div>
  <div class="body">
    <div class="layout-line">
      <span data-testid="action-layout">
        {layout ? `${layout.name} from ${layout.source}, ${layout.pages} pages, ${layout.tiles} tiles` : 'No layout loaded. Tiles resolve against the deck layout once connected.'}
      </span>
      <label class="file">
        Load layout file
        <input type="file" accept=".json,application/json" onchange={loadFile} data-testid="action-layout-file" />
      </label>
    </div>
    {#if problem}
      <p class="problem" data-testid="action-problem">{problem}</p>
    {/if}

    <h3>Key monitor</h3>
    <KeyMonitor />

    <h3>Action log</h3>
    <ol class="log-list" data-testid="action-log">
      {#each log as entry, i (i)}
        <li data-testid="action-entry" data-tile={entry.tile} data-status={entry.status} data-exit={entry.exit_code ?? ''} data-pid={entry.pid ?? ''}>
          <span class="when">{time(entry.at_ms)}</span>
          <span class="what">{entry.tile} {entry.status}{entry.exit_code === null ? '' : ` with ${entry.exit_code}`}{entry.pid === null ? '' : ` as pid ${entry.pid}`}</span>
          <span class="detail">{entry.detail}</span>
        </li>
      {:else}
        <li class="none">No tile actions yet.</li>
      {/each}
    </ol>
  </div>
</section>

<style>
  .actions {
    overflow: hidden;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    padding: 12px 14px;
    overflow: auto;
  }

  .layout-line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .file {
    flex: none;
    padding: 6px 14px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--raised);
    cursor: pointer;
  }

  .file input {
    display: none;
  }

  h3 {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .log-list {
    min-height: 80px;
    max-height: 180px;
    margin: 0;
    padding: 6px 8px;
    overflow: auto;
    border-radius: 8px;
    background: var(--raised);
    font-family: var(--mono);
    font-size: 12px;
    list-style: none;
  }

  .log-list li {
    display: flex;
    gap: 8px;
  }

  .when,
  .detail,
  .none {
    color: var(--muted);
  }

  .detail {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .problem {
    margin: 0;
    color: var(--bad);
  }
</style>
