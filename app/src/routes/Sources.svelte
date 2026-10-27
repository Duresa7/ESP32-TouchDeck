<script lang="ts">
  import { onMount } from 'svelte';
  import CliProxy from '../lib/sources/CliProxy.svelte';
  import { sourcesSave, sourcesStatus, sourcesView, type Config, type SourcesView, type Status } from '../lib/sources/api.ts';

  let view = $state<SourcesView | null>(null);
  let draft = $state<Config | null>(null);
  let status = $state<Status | null>(null);
  let problem = $state<string | null>(null);
  let saved = $state(false);

  const dirty = $derived(view !== null && draft !== null && JSON.stringify(view.config) !== JSON.stringify(draft));

  async function load() {
    try {
      view = await sourcesView();
      draft = structuredClone($state.snapshot(view.config));
    } catch (error) {
      problem = String(error);
    }
  }

  async function poll() {
    try {
      status = await sourcesStatus();
    } catch (error) {
      problem = String(error);
    }
  }

  async function save() {
    if (!draft) return;
    problem = null;
    try {
      view = await sourcesSave($state.snapshot(draft));
      draft = structuredClone($state.snapshot(view.config));
      saved = true;
      await poll();
    } catch (error) {
      problem = String(error);
    }
  }

  onMount(() => {
    load();
    poll();
    const timer = setInterval(poll, 1000);
    return () => clearInterval(timer);
  });
</script>

<section class="panel page" data-testid="sources-page">
  <div class="panel-head">
    <h2>Sources</h2>
    <span class="hint" data-testid="deck-feed" data-deck={status?.deck ?? false} data-sent={status?.sent ?? 0}>
      {status?.deck ? `Feeding the deck, ${status.sent} updates sent` : 'No deck connected'}
    </span>
  </div>

  <div class="body">
    <p class="note">Feed AI usage limits from your CLIProxyAPI hub to widget tiles and dashboard pages.</p>
    {#if view && draft}
      <section class="card" data-testid="layout-card">
        <h3>Tiles to feed</h3>
        <label class="row">
          <span>Layout file</span>
          <input
            bind:value={() => draft!.layout_path ?? '', (value) => (draft!.layout_path = value || null)}
            placeholder="the layout you pushed, such as dev.json"
            spellcheck="false"
            data-testid="layout-path"
          />
        </label>
        <p
          class="note"
          data-testid="layout-state"
          data-widgets={status?.layout.widgets ?? 0}
          data-dashboards={status?.layout.dashboards ?? 0}
          data-error={status?.layout.error ?? ''}
        >
          {#if status?.layout.error}
            {status.layout.error}
          {:else if status?.layout.name}
            {status.layout.name}: {status.layout.widgets} widget tile{status.layout.widgets === 1 ? '' : 's'} and {status.layout.dashboards} dashboard page{status.layout.dashboards === 1 ? '' : 's'}.
          {:else}
            No layout yet. Name the layout file the deck runs, so widget tiles and dashboard pages get their values.
          {/if}
        </p>
        {#if status?.send_error}<p class="bad" data-testid="send-error">{status.send_error}</p>{/if}
      </section>

      <CliProxy {view} {draft} info={status?.proxy} />

      <div class="save">
        <button class="primary" onclick={save} disabled={!dirty} data-testid="sources-save">Save</button>
        <span class="hint" data-testid="sources-saved">{saved && !dirty ? 'Saved' : dirty ? 'Unsaved changes' : ''}</span>
      </div>
    {/if}
    {#if problem}<p class="bad" data-testid="sources-error">{problem}</p>{/if}
  </div>
</section>

<style>
  .page {
    overflow: hidden;
  }
  .body {
    display: flex;
    flex-direction: column;
    flex: 1;
    gap: 14px;
    min-height: 0;
    padding: 14px;
    overflow-y: auto;
  }
  .card {
    padding: 14px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--raised);
  }
  h3 {
    margin: 0 0 6px;
    font-size: 15px;
  }
  .row {
    display: grid;
    grid-template-columns: 100px minmax(0, 1fr);
    align-items: center;
    gap: 8px;
  }
  .row input {
    min-width: 0;
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--bg);
    font-family: var(--mono);
    font-size: 12px;
  }
  .note {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: 13px;
  }
  .bad {
    color: var(--bad);
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .save {
    display: flex;
    align-items: center;
    gap: 12px;
  }
</style>
