<script lang="ts">
  import { connectable, connecting, connectTo, decks, listed, refresh, scanning } from '../lib/connection.ts';
  import DeckScreen from '../lib/deck/DeckScreen.svelte';
  import Inspector from '../lib/editor/Inspector.svelte';
  import LayoutMenu from '../lib/editor/LayoutMenu.svelte';
  import PageTabs from '../lib/editor/PageTabs.svelte';
  import { editor } from '../lib/editor/layoutStore.ts';
  import { plural } from '../lib/editor/messages.ts';
  import { counts, moveTile, resizeTile, type Edit, type Layout, type Orientation } from '../lib/editor/validate.ts';

  interface Props {
    online: boolean;
    onconnecthelp: () => void;
  }

  let { online, onconnecthelp }: Props = $props();

  let picker = $state<HTMLInputElement>();
  let mode = $state<'edit' | 'preview'>('edit');
  let actualSize = $state(false);

  const layout = $derived($editor.layouts[$editor.orientation]);
  const page = $derived(layout?.pages.find((p) => p.id === $editor.page) ?? null);
  const tile = $derived(page?.kind === 'grid' ? (page.tiles.find((t) => t.id === $editor.selected) ?? null) : null);
  const totals = $derived(layout ? counts(layout) : null);
  const plugged = $derived($decks.find((candidate) => connectable(candidate)) ?? null);

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
      editor.say('ok', `Saved “${name}”.`, { event: 'saved' });
    } catch (error) {
      if ((error as DOMException).name !== 'AbortError') editor.say('error', "Couldn't save the file. Pick another folder and try again.", { detail: String(error), event: 'save-failed' });
    }
  }

  function orient(orientation: Orientation) {
    editor.setOrientation(orientation);
  }

  function keys(event: KeyboardEvent) {
    const target = event.target as HTMLElement;
    if (target.closest('input, textarea, select, [contenteditable]')) return;
    if (event.key === 'p' && !event.ctrlKey && !event.metaKey && !event.altKey && layout) {
      mode = mode === 'edit' ? 'preview' : 'edit';
    }
  }
</script>

<svelte:window onkeydown={keys} />

<section class="editor" data-testid="editor" data-mode={mode}>
  <input type="file" accept=".json,application/json" hidden bind:this={picker} onchange={importFile} data-testid="import-file" />

  {#if !online}
    <div class="notice connect" data-tone={plugged ? 'info' : undefined} data-testid="connect-banner">
      {#if plugged}
        <p><strong>Your deck is plugged in.</strong> Connect to send your buttons to it.</p>
        <button class="small primary" onclick={() => connectTo(plugged.port)} disabled={$connecting !== null} data-testid="banner-connect"
          >{$connecting ? 'Connecting…' : 'Connect'}</button
        >
      {:else}
        <p>
          <strong>No deck connected.</strong>
          {$listed && !$scanning ? 'Plug your TouchDeck into this computer with a USB cable.' : 'Looking for your deck…'} You can still set up buttons now.
        </p>
        <button class="small" onclick={refresh} disabled={$scanning}>{$scanning ? 'Looking…' : 'Look again'}</button>
        <button class="small quiet" onclick={onconnecthelp}>Help</button>
      {/if}
    </div>
  {/if}

  {#if layout}
    <div class="toolbar">
      <div class="identity">
        <h1 class="name" data-testid="layout-name" title={layout.name}>{layout.name}</h1>
        <span class="count">{plural(totals?.pages ?? 0, 'page')} · {plural(totals?.tiles ?? 0, 'button')}</span>
      </div>
      <div class="segmented" role="group" aria-label="Mode">
        <button aria-pressed={mode === 'edit'} onclick={() => (mode = 'edit')} data-testid="mode-edit">Edit</button>
        <button aria-pressed={mode === 'preview'} onclick={() => (mode = 'preview')} data-testid="mode-preview" title="Try it like the deck (P)">Try it</button>
      </div>
      <div class="segmented" role="group" aria-label="Which way the deck stands">
        {#each ['landscape', 'portrait'] as const as orientation (orientation)}
          <button
            aria-pressed={$editor.orientation === orientation}
            onclick={() => orient(orientation)}
            data-testid="orientation-{orientation}"
            title={orientation === 'landscape' ? 'Wide' : 'Tall'}
          >
            <svg class="orient" viewBox="0 0 20 20" aria-hidden="true">
              {#if orientation === 'landscape'}<rect x="2.5" y="5.5" width="15" height="9" rx="2" />{:else}<rect x="5.5" y="2.5" width="9" height="15" rx="2" />{/if}
            </svg>
            {orientation === 'landscape' ? 'Landscape' : 'Portrait'}
          </button>
        {/each}
      </div>
      <div class="end">
        <LayoutMenu {online} busy={$editor.busy !== null} onopen={() => picker?.click()} onsave={exportFile} />
        <button
          class="primary"
          onclick={() => editor.push()}
          disabled={!online || $editor.busy !== null}
          title={online ? 'Send these buttons to your deck' : 'Connect your deck first'}
          data-testid="push">{$editor.busy === 'push' ? 'Sending…' : 'Send to deck'}</button
        >
      </div>
    </div>

    {#if $editor.pulled}
      <div class="notice confirm" data-tone="info" data-testid="pull-confirm">
        <p>
          Your deck has <strong>“{$editor.pulled.layout.name}”</strong> ({plural(counts($editor.pulled.layout).pages, 'page')}, {plural(
            counts($editor.pulled.layout).tiles,
            'button',
          )}). Replace the {$editor.pulled.orientation} layout here with it?
        </p>
        <button class="small primary" onclick={() => editor.confirmPull()} data-testid="pull-replace">Replace</button>
        <button class="small" onclick={() => editor.cancelPull()} data-testid="pull-cancel">Keep mine</button>
      </div>
    {/if}

    <div class="pagebar">
      <PageTabs pages={layout.pages} current={$editor.page} editing={mode === 'edit'} onselect={(id) => editor.selectPage(id)} onadd={() => editor.addPage()} />
      {#if mode === 'edit' && page?.kind === 'grid'}
        <button class="small" onclick={() => editor.addTile()} data-testid="add-tile">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3 8h10" /></svg>
          Add button
        </button>
      {/if}
    </div>

    <div class="work" class:previewing={mode === 'preview'}>
      <div class="stage">
        {#if page}
          <DeckScreen
            {layout}
            page={page.id}
            {mode}
            {actualSize}
            selected={$editor.selected}
            onpage={(id) => editor.selectPage(id)}
            onselect={(id) => editor.select(id)}
            onmove={(id, row, col) => editor.apply((l) => moveTile(l, page.id, id, row, col))}
            onresize={(id, rowSpan, colSpan) => editor.apply((l) => resizeTile(l, page.id, id, rowSpan, colSpan))}
            onadd={(row, col) => editor.addTile([row, col])}
          />
        {/if}
        <div class="stage-foot">
          {#if mode === 'preview'}
            <p class="hint">Swipe sideways or press ← and → to change pages, and tap a button to see it react, exactly as on the deck.</p>
            <label class="check small-check">
              <input type="checkbox" bind:checked={actualSize} data-testid="actual-size" />
              Actual pixels
            </label>
          {:else if page?.kind === 'grid'}
            <p class="hint">Click a button to change it, or an empty square to add one. Drag to move, and drag the corner dot to resize.</p>
          {:else}
            <p class="hint">An AI usage page fills itself with your accounts once AI usage is set up. These rows are examples.</p>
          {/if}
        </div>
      </div>
      {#if mode === 'edit'}
        <aside class="inspector" aria-label="Details">
          <Inspector {layout} {page} {tile} {apply} />
        </aside>
      {/if}
    </div>
  {:else}
    <div class="welcome" data-testid="welcome">
      <div class="ghost" aria-hidden="true">
        {#each Array.from({ length: 12 }) as _, i (i)}<span></span>{/each}
      </div>
      <h1>Set up your buttons</h1>
      <p class="lead">Choose what each button on your TouchDeck does. You'll see every page exactly as the deck will show it.</p>
      <div class="starts">
        <button class="start" onclick={() => editor.pull()} disabled={!online || $editor.busy !== null} data-testid="empty-pull">
          <strong>Start from your deck</strong>
          <span>{online ? 'Load the buttons your deck has now.' : 'Connect your deck to load its buttons.'}</span>
        </button>
        <button class="start" onclick={() => editor.startStarter()} data-testid="empty-starter">
          <strong>Start with examples</strong>
          <span>Media controls and everyday shortcuts, ready to change.</span>
        </button>
        <button class="start" onclick={() => editor.startNew()} data-testid="new-layout">
          <strong>Start blank</strong>
          <span>One empty page with room for 12 buttons.</span>
        </button>
      </div>
      <button class="quiet" onclick={() => picker?.click()} data-testid="import">Open a layout file…</button>
    </div>
  {/if}

  <footer class="statusbar" data-testid="editor-status" data-kind={$editor.status?.kind ?? 'none'} data-event={$editor.status?.event ?? ''} data-detail={$editor.status?.detail ?? ''} data-pages={$editor.status?.pages ?? ''} data-tiles={$editor.status?.tiles ?? ''} role="status">
    {#if $editor.status}
      <span class="message" data-testid="status-message">{$editor.status.text}</span>
      {#if $editor.status.detail && $editor.status.kind === 'error'}
        <details class="more inline"><summary>Details</summary><div class="body">{$editor.status.detail}</div></details>
      {/if}
    {/if}
  </footer>
</section>

<style>
  .editor {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
    padding: var(--space-4) var(--space-5) var(--space-2);
  }

  .connect {
    align-items: center;
  }

  .connect p {
    flex: 1;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .identity {
    display: flex;
    flex: 1;
    align-items: baseline;
    gap: var(--space-3);
    min-width: 0;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-xl);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    flex: none;
    color: var(--muted);
    font-size: var(--text-s);
  }

  .orient {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
  }

  .end {
    display: flex;
    flex: none;
    gap: var(--space-2);
  }

  .confirm {
    align-items: center;
  }

  .pagebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-width: 0;
  }

  .pagebar > :global(.tabs) {
    flex: 1;
  }

  .pagebar svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
    stroke-width: 1.8;
  }

  .confirm p {
    flex: 1;
  }

  .work {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) 340px;
    gap: var(--space-4);
    min-height: 0;
  }

  .work.previewing {
    grid-template-columns: minmax(0, 1fr);
  }

  .stage {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    gap: var(--space-2);
    min-width: 0;
    min-height: 0;
    border-radius: var(--radius-l);
    background: radial-gradient(120% 90% at 50% 45%, #182032 0%, rgb(18 23 34 / 0) 70%);
  }

  .stage-foot {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-4);
    min-height: 28px;
    padding: 0 var(--space-3);
  }

  .stage-foot .hint {
    text-align: center;
  }

  .small-check {
    flex: none;
    color: var(--muted);
    font-size: var(--text-s);
  }

  .inspector {
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-4);
    border: 1px solid var(--line);
    border-radius: var(--radius-l);
    background: var(--panel);
  }

  .welcome {
    display: grid;
    flex: 1;
    align-content: center;
    justify-items: center;
    gap: var(--space-4);
    padding: var(--space-5);
    text-align: center;
  }

  .ghost {
    display: grid;
    grid-template-columns: repeat(4, 34px);
    gap: 7px;
    padding: 16px 18px 20px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: #080810;
    margin-bottom: var(--space-2);
  }

  .ghost span {
    height: 24px;
    border-radius: 6px;
    background: #182031;
  }

  .ghost span:nth-child(7) {
    background: #29416b;
    box-shadow: inset 0 0 0 1.5px #4aa2ff;
  }

  .lead {
    max-width: 52ch;
    color: var(--muted);
    font-size: var(--text-l);
  }

  .starts {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 220px));
    gap: var(--space-3);
    margin-top: var(--space-2);
  }

  .start {
    display: grid;
    align-content: start;
    justify-items: start;
    gap: 6px;
    height: auto;
    min-height: 96px;
    padding: var(--space-4);
    border-color: var(--line);
    border-radius: var(--radius-m);
    background: var(--panel);
    text-align: left;
    white-space: normal;
  }

  .start strong {
    font-family: var(--font-display);
    font-size: var(--text-l);
    font-weight: 500;
  }

  .start span {
    color: var(--muted);
    font-size: var(--text-s);
    font-weight: 400;
    line-height: 1.45;
  }

  .statusbar {
    display: flex;
    flex: none;
    align-items: baseline;
    gap: var(--space-3);
    min-height: 26px;
    color: var(--muted);
    font-size: var(--text-s);
  }

  .message {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .statusbar[data-kind='error'] .message {
    color: var(--bad);
  }

  .statusbar[data-kind='ok'] .message {
    color: var(--good);
  }

  .inline {
    flex: none;
  }

  .inline[open] {
    flex: 1;
  }
</style>
