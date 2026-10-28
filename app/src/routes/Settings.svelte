<script lang="ts" module>
  export type Section = 'deck' | 'software' | 'app' | 'help';
</script>

<script lang="ts">
  import { deck, online } from '../lib/connection.ts';
  import Settings from '../lib/settings/Settings.svelte';
  import Troubleshooting from '../lib/settings/Troubleshooting.svelte';
  import AppUpdates from '../lib/updates/AppUpdates.svelte';
  import Update from './Update.svelte';

  let { section = $bindable('deck'), title }: { section?: Section; title: string } = $props();

  const SECTIONS: [Section, string, string][] = [
    ['deck', 'Your deck', 'Screen and orientation'],
    ['software', 'Deck software', 'Update or repair the deck'],
    ['app', 'This app', 'Version and updates'],
    ['help', 'Troubleshooting', 'Connection, testing, and logs'],
  ];
</script>

<div class="settings-page">
  <nav class="sections" aria-label="Settings">
    {#each SECTIONS as [id, name, note] (id)}
      <button class="quiet" class:active={section === id} onclick={() => (section = id)} aria-current={section === id ? 'page' : undefined} data-testid="settings-tab-{id}">
        <span class="name">{name}</span>
        <span class="note">{note}</span>
      </button>
    {/each}
  </nav>
  <div class="content" data-testid="settings-content">
    {#if section === 'software'}
      <Update port={$deck?.port ?? ''} />
    {:else if section === 'app'}
      <AppUpdates online={$online} />
    {:else if section === 'help'}
      <Troubleshooting {title} />
    {:else}
      <Settings online={$online} session={$online && $deck ? $deck.session : null} />
    {/if}
  </div>
</div>

<style>
  .settings-page {
    display: grid;
    flex: 1;
    grid-template-columns: 240px minmax(0, 1fr);
    min-height: 0;
  }

  .sections {
    display: grid;
    align-content: start;
    gap: 2px;
    padding: var(--space-5) var(--space-3);
    border-right: 1px solid var(--line);
  }

  .sections button {
    display: grid;
    justify-content: start;
    justify-items: start;
    gap: 2px;
    height: auto;
    padding: 10px var(--space-3);
    text-align: left;
  }

  .sections .name {
    color: var(--text);
    font-weight: 500;
  }

  .sections .note {
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 400;
  }

  .sections button.active {
    background: var(--raised);
  }

  .sections button.active .name {
    color: var(--accent);
  }

  .content {
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-5) var(--space-6) var(--space-7);
  }

  .content > :global(*) {
    max-width: 760px;
  }
</style>
