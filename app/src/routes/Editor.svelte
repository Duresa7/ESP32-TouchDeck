<script lang="ts">
  import Grid from '../lib/editor/Grid.svelte';
  import PageTabs from '../lib/editor/PageTabs.svelte';
  import TileProps from '../lib/editor/TileProps.svelte';
  import { editor } from '../lib/editor/layoutStore.ts';
  import { counts, moveTile, resizeTile, type Edit, type Layout, type Orientation } from '../lib/editor/validate.ts';

  interface Props {
    online: boolean;
  }

  let { online }: Props = $props();

  let picker = $state<HTMLInputElement>();

  const layout = $derived($editor.layouts[$editor.orientation]);
  const page = $derived(layout?.pages.find((p) => p.id === $editor.page) ?? null);
  const tile = $derived(page?.kind === 'grid' ? (page.tiles.find((t) => t.id === $editor.selected) ?? null) : null);
  const pageList = $derived(layout?.pages.map((p) => ({ id: p.id, title: p.title })) ?? []);
  const totals = $derived(layout ? counts(layout) : null);

  const apply = (edit: (layout: Layout) => Edit) => editor.apply(edit);

  async function importFile(event: Event & { currentTarget: HTMLInputElement }) {
    const input = event.currentTarget;
    const file = input.files?.[0];
    if (!file) return;
    editor.importText(await file.text(), file.name);
    input.value = '';
  }

  type SavePicker = (options: object) => Promise<{ createWritable: () => Promise<{ write: (data: string) => Promise<void>; close: () => Promise<void> }> }>;

  async function exportFile() {
    const text = editor.exportText();
    if (!text || !layout) return;
    const name = `${layout.name.trim().replace(/[^A-Za-z0-9._-]+/g, '-') || 'layout'}.json`;
    const save = (window as unknown as { showSaveFilePicker?: SavePicker }).showSaveFilePicker;
    try {
      if (save) {
        const handle = await save({ suggestedName: name, types: [{ description: 'TouchDeck layout', accept: { 'application/json': ['.json'] } }] });
        const writable = await handle.createWritable();
        await writable.write(text);
        await writable.close();
      } else {
        const link = document.createElement('a');
        link.href = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
        link.download = name;
        link.click();
        URL.revokeObjectURL(link.href);
      }
      editor.say('ok', `Exported ${name}`);
    } catch (error) {
      if ((error as DOMException).name !== 'AbortError') editor.say('error', `Export failed: ${String(error)}`);
    }
  }

  function orient(orientation: Orientation) {
    editor.setOrientation(orientation);
  }
</script>

<section class="editor" data-testid="editor">
  <div class="toolbar">
    <div class="segmented" role="group" aria-label="Orientation">
      {#each ['landscape', 'portrait'] as const as orientation (orientation)}
        <button
          class:active={$editor.orientation === orientation}
          onclick={() => orient(orientation)}
          data-testid="orientation-{orientation}"
          aria-pressed={$editor.orientation === orientation}>{orientation === 'landscape' ? 'Landscape' : 'Portrait'}</button
        >
      {/each}
    </div>
    <span class="name" data-testid="layout-name">{layout ? `${layout.name} · ${totals?.pages} pages, ${totals?.tiles} tiles` : 'No layout open'}</span>
    <div class="actions">
      <button onclick={() => editor.startNew()} data-testid="new-layout">New</button>
      <button onclick={() => picker?.click()} data-testid="import">Import</button>
      <input type="file" accept=".json,application/json" hidden bind:this={picker} onchange={importFile} data-testid="import-file" />
      <button onclick={exportFile} disabled={!layout} data-testid="export">Export</button>
      <button onclick={() => editor.pull()} disabled={!online || $editor.busy !== null} data-testid="pull">{$editor.busy === 'pull' ? 'Pulling' : 'Pull'}</button>
      <button class="primary" onclick={() => editor.push()} disabled={!online || !layout || $editor.busy !== null} data-testid="push"
        >{$editor.busy === 'push' ? 'Pushing' : 'Push'}</button
      >
    </div>
  </div>

  {#if $editor.pulled}
    <div class="confirm" data-testid="pull-confirm">
      <span>Replace the editor's {$editor.pulled.orientation} layout with the deck's {$editor.pulled.layout.name}?</span>
      <button class="primary" onclick={() => editor.confirmPull()} data-testid="pull-replace">Replace</button>
      <button onclick={() => editor.cancelPull()} data-testid="pull-cancel">Cancel</button>
    </div>
  {/if}

  {#if layout}
    <PageTabs
      pages={layout.pages}
      current={$editor.page}
      onselect={(id) => editor.selectPage(id)}
      onadd={() => editor.addPage()}
      onremove={(id) => editor.removePage(id)}
    />
    <div class="work">
      <div class="canvas">
        {#if page?.kind === 'grid'}
          <Grid
            {page}
            orientation={layout.orientation}
            selected={$editor.selected}
            onselect={(id) => editor.select(id)}
            onmove={(id, row, col) => editor.apply((l) => moveTile(l, page.id, id, row, col))}
            onresize={(id, rowSpan, colSpan) => editor.apply((l) => resizeTile(l, page.id, id, rowSpan, colSpan))}
          />
        {:else if page}
          <div class="dashboard" data-testid="dashboard-page">Dashboard page fed by <code>{page.source}</code>. The deck draws it as one panel.</div>
        {/if}
      </div>
      <aside class="side">
        {#if page?.kind === 'grid'}
          <button onclick={() => editor.addTile()} data-testid="add-tile">Add tile</button>
        {/if}
        {#if tile && page}
          <TileProps page={page.id} {tile} pages={pageList} {apply} onremove={() => editor.removeTile(tile.id)} />
        {:else}
          <p class="hint">Select a tile to edit its label, icon, and action. Drag a tile to move it, and drag its corner to resize it.</p>
        {/if}
      </aside>
    </div>
  {:else}
    <div class="empty">
      <p>No {$editor.orientation} layout is open. Import a layout file, pull the deck's layout, or start a new one.</p>
    </div>
  {/if}

  <footer class="statusbar" data-testid="editor-status" data-kind={$editor.status?.kind ?? 'none'}>
    {$editor.status?.text ?? ''}
  </footer>
</section>

<style>
  .editor {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    padding: 16px;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions,
  .segmented {
    display: flex;
    gap: 6px;
  }

  .segmented button.active {
    border-color: var(--accent);
    color: #fff;
  }

  .confirm {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid #3a475d;
    border-radius: 10px;
    background: var(--raised);
  }

  .confirm span {
    flex: 1;
  }

  .work {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) minmax(240px, 300px);
    gap: 16px;
    min-height: 0;
  }

  .canvas {
    min-width: 0;
    min-height: 0;
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    padding: 12px;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
  }

  .hint,
  .empty,
  .dashboard {
    color: var(--muted);
  }

  .empty,
  .dashboard {
    display: grid;
    flex: 1;
    place-items: center;
    border: 1px dashed var(--line);
    border-radius: 12px;
    text-align: center;
  }

  .dashboard {
    height: 100%;
  }

  .statusbar {
    min-height: 22px;
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .statusbar[data-kind='error'] {
    color: var(--bad);
  }

  .statusbar[data-kind='ok'] {
    color: var(--online);
  }
</style>
