<script lang="ts">
  import { PAGES_MAX, type Page } from './validate.ts';

  interface Props {
    pages: Page[];
    current: string | null;
    onselect: (page: string) => void;
    onadd: () => void;
    onremove: (page: string) => void;
  }

  let { pages, current, onselect, onadd, onremove }: Props = $props();

  let arming = $state<string | null>(null);

  function remove() {
    if (!current) return;
    if (arming === current) {
      arming = null;
      onremove(current);
    } else {
      arming = current;
    }
  }
</script>

<div class="tabs" data-testid="page-tabs">
  <div class="list" role="tablist">
    {#each pages as page (page.id)}
      <button
        class="tab"
        class:active={page.id === current}
        role="tab"
        aria-selected={page.id === current}
        onclick={() => {
          arming = null;
          onselect(page.id);
        }}
        data-testid="page-tab"
        data-id={page.id}
        data-kind={page.kind}
      >
        {page.title}
        <span class="count">{page.kind === 'grid' ? page.tiles.length : 'dash'}</span>
      </button>
    {/each}
  </div>
  <button onclick={onadd} disabled={pages.length >= PAGES_MAX} data-testid="add-page">Add page</button>
  <button class:danger={arming !== null} onclick={remove} disabled={!current || pages.length <= 1} data-testid="remove-page">
    {arming !== null && arming === current ? 'Click to remove' : 'Remove page'}
  </button>
</div>

<style>
  .tabs {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .list {
    display: flex;
    flex: 1;
    gap: 4px;
    min-width: 0;
    overflow-x: auto;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }

  .tab.active {
    border-color: var(--line);
    background: var(--raised);
    color: var(--text);
  }

  .count {
    padding: 0 6px;
    border-radius: 999px;
    background: #0f141d;
    color: var(--muted);
    font-size: 11px;
  }

  .danger {
    color: var(--bad);
  }
</style>
