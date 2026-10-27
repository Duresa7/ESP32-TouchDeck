<script lang="ts">
  import { PAGES_MAX, type Page } from './validate.ts';

  interface Props {
    pages: Page[];
    current: string | null;
    editing: boolean;
    onselect: (page: string) => void;
    onadd: () => void;
  }

  let { pages, current, editing, onselect, onadd }: Props = $props();

  function keys(event: KeyboardEvent) {
    const at = pages.findIndex((page) => page.id === current);
    const to = { ArrowLeft: at - 1, ArrowRight: at + 1, Home: 0, End: pages.length - 1 }[event.key];
    if (to === undefined || to < 0 || to >= pages.length) return;
    event.preventDefault();
    onselect(pages[to].id);
    (event.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-id="${pages[to].id}"]`)?.focus();
  }
</script>

<div class="tabs" data-testid="page-tabs">
  <div class="list" role="tablist" aria-label="Pages" tabindex="-1" onkeydown={keys}>
    {#each pages as page, i (page.id)}
      <button
        class="tab"
        class:active={page.id === current}
        role="tab"
        aria-selected={page.id === current}
        tabindex={page.id === current ? 0 : -1}
        onclick={() => onselect(page.id)}
        data-testid="page-tab"
        data-id={page.id}
        data-kind={page.kind}
        title={page.title}
      >
        <span class="number" aria-hidden="true">{i + 1}</span>
        <span class="title">{page.title}</span>
        {#if page.kind === 'dashboard'}<span class="tag">AI usage</span>{/if}
      </button>
    {/each}
  </div>
  {#if editing}
    <button class="small quiet add" onclick={onadd} disabled={pages.length >= PAGES_MAX} data-testid="add-page" title={pages.length >= PAGES_MAX ? `A layout can have up to ${PAGES_MAX} pages` : 'Add a page'}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3 8h10" /></svg>
      Add page
    </button>
  {/if}
</div>

<style>
  .tabs {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .list {
    display: flex;
    gap: var(--space-1);
    min-width: 0;
    overflow-x: auto;
    padding: 2px;
  }

  .tab {
    gap: var(--space-2);
    max-width: 220px;
    min-height: 32px;
    padding: 0 var(--space-3) 0 6px;
    border-color: transparent;
    border-radius: 999px;
    background: transparent;
    color: var(--muted);
  }

  .tab.active {
    border-color: var(--line-strong);
    background: var(--raised);
    color: var(--text);
  }

  .number {
    display: grid;
    flex: none;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--sunken);
    color: var(--muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .tab.active .number {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tag {
    flex: none;
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
  }

  .add {
    flex: none;
  }

  .add svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
    stroke-width: 1.8;
  }
</style>
