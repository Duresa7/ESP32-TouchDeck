<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { builtinIcons, fileId, iconUrls, loadIcon, searchIcons, type BuiltinIcon } from './icons.ts';
  import { editor } from './layoutStore.ts';
  import { setIcon, type Edit, type Layout, type Tile } from './validate.ts';

  interface Props {
    page: string;
    tile: Tile;
    apply: (edit: (layout: Layout) => Edit) => string | null;
  }

  let { page, tile, apply }: Props = $props();

  let open = $state(false);
  let query = $state('');
  let icons = $state<BuiltinIcon[]>([]);
  let suggested = $state<BuiltinIcon | null>(null);
  let problem = $state<string | null>(null);
  let taking = $state(false);

  const current = $derived(fileId(tile.icon));
  const shown = $derived(searchIcons(icons, query));

  $effect(() => {
    builtinIcons()
      .then((list) => {
        icons = list;
        for (const icon of list) loadIcon(icon.id);
      })
      .catch((error) => (problem = `Built-in icons: ${String(error)}`));
  });

  $effect(() => {
    const action = tile.action;
    invoke<BuiltinIcon | null>('default_icon', { action })
      .then((icon) => {
        suggested = icon;
        if (icon) loadIcon(icon.id);
      })
      .catch(() => (suggested = null));
  });

  $effect(() => {
    if (current) loadIcon(current);
  });

  function use(id: string) {
    problem = apply((l) => setIcon(l, page, tile.id, `file:${id}`));
  }

  async function useAppIcon() {
    if (tile.action.type !== 'launch') return;
    taking = true;
    problem = null;
    try {
      const id = await invoke<string>('take_app_icon', { path: tile.action.path });
      loadIcon(id);
      use(id);
    } catch (error) {
      problem = `No app icon: ${String(error)}`;
    } finally {
      taking = false;
    }
  }
</script>

<div class="picker" data-testid="icon-picker">
  <div class="row">
    <span class="preview" data-testid="icon-preview" data-id={current ?? ''}>
      {#if current && $iconUrls[current]}
        <img src={$iconUrls[current]} alt="" />
      {/if}
    </span>
    <button onclick={() => (open = !open)} data-testid="icon-picker-toggle">{open ? 'Hide icons' : 'Choose icon'}</button>
    {#if tile.action.type === 'launch'}
      <button onclick={useAppIcon} disabled={taking || !tile.action.path} data-testid="use-app-icon">{taking ? 'Taking icon' : 'Use app icon'}</button>
    {/if}
    {#if suggested && suggested.id !== current}
      <button class="suggest" onclick={() => use(suggested!.id)} title="Suggested for this action" data-testid="icon-suggested" data-name={suggested.name}>
        {#if $iconUrls[suggested.id]}<img src={$iconUrls[suggested.id]} alt="" />{/if}
        {suggested.label}
      </button>
    {/if}
  </div>

  {#if open}
    <input type="search" placeholder="Search icons" bind:value={query} data-testid="icon-search" />
    <div class="grid" data-testid="icon-grid" data-count={shown.length}>
      {#each shown as icon (icon.name)}
        <button
          class="option"
          class:chosen={icon.id === current}
          onclick={() => use(icon.id)}
          title={icon.label}
          data-testid="icon-option"
          data-name={icon.name}
          data-id={icon.id}
        >
          {#if $iconUrls[icon.id]}<img src={$iconUrls[icon.id]} alt={icon.label} />{/if}
          <span>{icon.name}</span>
        </button>
      {:else}
        <p class="empty">No icon matches "{query}"</p>
      {/each}
    </div>
    <button class="prune" onclick={() => editor.removeUnusedIcons()} data-testid="remove-unused-icons">Remove unused icons</button>
  {/if}

  {#if problem}
    <p class="problem" data-testid="icon-error">{problem}</p>
  {/if}
</div>

<style>
  .picker {
    display: grid;
    gap: 8px;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .preview {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 1px dashed #2f3b50;
    border-radius: 8px;
    background: #111827;
  }

  .preview img {
    width: 32px;
    height: 32px;
  }

  .suggest {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .suggest img {
    width: 18px;
    height: 18px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(64px, 1fr));
    gap: 6px;
    max-height: 240px;
    overflow-y: auto;
    padding: 2px;
  }

  .option {
    display: grid;
    justify-items: center;
    gap: 2px;
    padding: 6px 2px;
    min-width: 0;
  }

  .option img {
    width: 32px;
    height: 32px;
  }

  .option span {
    max-width: 100%;
    overflow: hidden;
    color: var(--muted);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .option.chosen {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px rgb(59 130 246 / 0.35);
  }

  .empty {
    grid-column: 1 / -1;
    margin: 0;
    color: var(--muted);
    font-size: 12px;
  }

  .prune {
    justify-self: start;
  }

  .problem {
    margin: 0;
    color: var(--bad);
    font-size: 12px;
  }
</style>
