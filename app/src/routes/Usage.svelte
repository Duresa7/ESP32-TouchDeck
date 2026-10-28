<script lang="ts">
  import { onMount } from 'svelte';
  import { plural } from '../lib/editor/messages.ts';
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

<section class="usage" data-testid="sources-page">
  <div class="column">
    <header class="intro">
      <h1>AI usage</h1>
      <p class="lead">
        Show how much of your AI plans you've used, right on the deck. Add a button that shows AI usage, or an AI usage page, and TouchDeck keeps
        its numbers up to date while it runs.
      </p>
      <p class="feed" data-testid="deck-feed" data-deck={status?.deck ?? false} data-sent={status?.sent ?? 0}>
        <span class="dot" data-on={status?.deck ?? false} aria-hidden="true"></span>
        {status?.deck ? `Sending usage to your deck: ${plural(status.sent, 'update')} so far.` : 'Your deck isn’t connected, so nothing is being sent.'}
      </p>
    </header>

    {#if view && draft}
      <CliProxy {view} {draft} info={status?.proxy} />

      <div class="section" data-testid="layout-card">
        <header>
          <h3>Which buttons to fill</h3>
          <p class="hint">TouchDeck fills the AI usage buttons and pages of the layout on your deck. To fill the ones in a saved layout file instead, name the file here.</p>
        </header>
        <label class="field">
          <span class="name">Layout file, optional</span>
          <input
            bind:value={() => draft!.layout_path ?? '', (value) => (draft!.layout_path = value || null)}
            placeholder="Leave empty to use the buttons on your deck"
            spellcheck="false"
            data-testid="layout-path"
          />
        </label>
        <p
          class="hint"
          data-testid="layout-state"
          data-widgets={status?.layout.widgets ?? 0}
          data-dashboards={status?.layout.dashboards ?? 0}
          data-error={status?.layout.error ?? ''}
        >
          {#if status?.layout.error}
            <span class="bad">That layout couldn't be read, so no buttons are filled. Check the file name.</span>
            <span class="raw">{status.layout.error}</span>
          {:else if status?.layout.name}
            “{status.layout.name}” has {plural(status.layout.widgets, 'AI usage button')} and {plural(status.layout.dashboards, 'AI usage page')}.
          {:else}
            No layout yet. Connect your deck, or name a layout file.
          {/if}
        </p>
        {#if status?.send_error}<p class="problem" data-testid="send-error">Couldn't send the numbers to your deck: {status.send_error}</p>{/if}
      </div>

      <div class="save">
        <button class="primary" onclick={save} disabled={!dirty} data-testid="sources-save">Save</button>
        <span class="hint" data-testid="sources-saved">{saved && !dirty ? 'Saved' : dirty ? 'You have changes to save' : ''}</span>
      </div>
    {/if}
    {#if problem}
      <div class="notice" data-tone="bad">
        <div>
          <p>Something went wrong with AI usage. Try again, or restart TouchDeck.</p>
          <details class="more"><summary>Details</summary><div class="body" data-testid="sources-error">{problem}</div></details>
        </div>
      </div>
    {/if}
  </div>
</section>

<style>
  .usage {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-5) var(--space-6) var(--space-7);
  }

  .column {
    display: grid;
    gap: var(--space-5);
    max-width: 860px;
  }

  .intro {
    display: grid;
    gap: var(--space-3);
  }

  .lead {
    max-width: 68ch;
    color: var(--muted);
    font-size: var(--text-l);
  }

  .feed {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--muted);
    font-size: var(--text-s);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint);
  }

  .dot[data-on='true'] {
    background: var(--good);
  }

  .bad {
    color: var(--bad);
  }

  .raw {
    display: block;
    margin-top: 4px;
    font-family: var(--mono);
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }

  .save {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
</style>
