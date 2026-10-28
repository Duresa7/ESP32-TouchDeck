<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import Approval from './lib/actions/Approval.svelte';
  import { online, start } from './lib/connection.ts';
  import ConnectionMenu from './lib/shell/ConnectionMenu.svelte';
  import Editor from './routes/Editor.svelte';
  import Settings, { type Section } from './routes/Settings.svelte';
  import Usage from './routes/Usage.svelte';

  type View = 'buttons' | 'usage' | 'settings';

  let title = $state('Open TouchDeck');
  let buildSha = $state('');
  let view = $state<View>('buttons');
  let section = $state<Section>('deck');

  const VIEWS: [View, string][] = [
    ['buttons', 'Buttons'],
    ['usage', 'AI usage'],
    ['settings', 'Settings'],
  ];

  function go(to: View, at?: Section) {
    view = to;
    if (at) section = at;
  }

  onMount(() => {
    const stop = start();
    invoke<string>('build_sha').then((sha) => (buildSha = sha));
    getCurrentWindow()
      .title()
      .then((name) => {
        title = name;
        document.title = name;
      });
    return stop;
  });
</script>

<header class="bar">
  <div class="brand" data-testid="title" title={title}>
    <svg class="mark" viewBox="0 0 28 20" aria-hidden="true">
      <rect x="0.75" y="0.75" width="26.5" height="18.5" rx="4.5" />
      <rect class="key" x="5" y="5" width="5" height="4" rx="1.2" />
      <rect class="key" x="11.5" y="5" width="5" height="4" rx="1.2" />
      <rect class="key lit" x="18" y="5" width="5" height="4" rx="1.2" />
      <rect class="key" x="5" y="11" width="5" height="4" rx="1.2" />
      <rect class="key" x="11.5" y="11" width="5" height="4" rx="1.2" />
      <rect class="key" x="18" y="11" width="5" height="4" rx="1.2" />
    </svg>
    <span>Open TouchDeck</span>
  </div>
  <nav class="views" aria-label="Main">
    {#each VIEWS as [id, name] (id)}
      <button class="quiet" class:active={view === id} onclick={() => go(id)} aria-current={view === id ? 'page' : undefined} data-testid="nav-{id}"
        >{name}</button
      >
    {/each}
  </nav>
  <ConnectionMenu onhelp={() => go('settings', 'help')} />
</header>

<main class="view">
  {#if view === 'usage'}
    <Usage />
  {:else if view === 'settings'}
    <Settings bind:section {buildSha} />
  {:else}
    <Editor online={$online} onconnecthelp={() => go('settings', 'help')} />
  {/if}
</main>

<Approval />

<style>
  .bar {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--space-5);
    height: 56px;
    padding: 0 var(--space-4) 0 var(--space-5);
    border-bottom: 1px solid var(--line);
    background: var(--bg);
  }

  .brand {
    display: flex;
    flex: none;
    align-items: center;
    gap: 10px;
    font-family: var(--font-display);
    font-size: 17px;
    font-weight: 500;
    letter-spacing: -0.01em;
  }

  .mark {
    width: 28px;
    height: 20px;
    fill: none;
    stroke: var(--line-strong);
    stroke-width: 1.5;
  }

  .mark .key {
    fill: var(--raised);
    stroke: none;
  }

  .mark .lit {
    fill: var(--accent);
  }

  .views {
    display: flex;
    flex: 1;
    gap: var(--space-1);
  }

  .views button {
    position: relative;
    min-height: 36px;
    padding: 0 var(--space-3);
    color: var(--muted);
    font-size: var(--text-m);
  }

  .views button.active {
    color: var(--text);
  }

  .views button.active::after {
    position: absolute;
    right: var(--space-3);
    bottom: -11px;
    left: var(--space-3);
    height: 2px;
    border-radius: 2px;
    background: var(--accent);
    content: '';
  }

  .view {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }
</style>
