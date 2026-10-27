<script lang="ts">
  import { missingGlyphs } from '../deck/scene.ts';
  import { editor } from './layoutStore.ts';
  import { friendlyRefusal } from './messages.ts';
  import TileProps from './TileProps.svelte';
  import { setFullScreen, setGridSize, setLayoutName, setPageTitle, type Edit, type Layout, type Page, type Tile } from './validate.ts';

  interface Props {
    layout: Layout;
    page: Page | null;
    tile: Tile | null;
    apply: (edit: (layout: Layout) => Edit) => string | null;
  }

  let { layout, page, tile, apply }: Props = $props();

  let title = $state('');
  let name = $state('');
  let titleProblem = $state<string | null>(null);
  let nameProblem = $state<string | null>(null);
  let arming = $state<string | null>(null);

  $effect.pre(() => {
    title = page?.title ?? '';
    titleProblem = null;
  });

  $effect.pre(() => {
    name = layout.name;
    nameProblem = null;
  });

  const pages = $derived(layout.pages.map((p) => ({ id: p.id, title: p.title })));
  const SIZES = [1, 2, 3, 4, 5, 6, 7, 8];
  const unseen = $derived([...missingGlyphs(title), ...missingGlyphs(name)]);

  function rename(value: string) {
    if (!page) return;
    const reason = apply((l) => setPageTitle(l, page.id, value));
    titleProblem = reason ? friendlyRefusal(reason) : null;
  }

  function renameLayout(value: string) {
    const reason = apply((l) => setLayoutName(l, value));
    nameProblem = reason ? friendlyRefusal(reason) : null;
  }

  function resize(rows: number, cols: number) {
    if (page) apply((l) => setGridSize(l, page.id, rows, cols));
  }

  function remove() {
    if (!page) return;
    if (arming === page.id) {
      arming = null;
      editor.removePage(page.id);
    } else {
      arming = page.id;
    }
  }
</script>

{#if tile && page}
  <TileProps page={page.id} {tile} {pages} {apply} onremove={() => editor.removeTile(tile.id)} />
{:else if page}
  <div class="panel" data-testid="page-props" data-id={page.id}>
    <section>
      <header>
        <span class="kicker">Page {layout.pages.indexOf(page) + 1} of {layout.pages.length}</span>
        <h2>{page.kind === 'grid' ? 'This page' : 'AI usage page'}</h2>
      </header>
      <label class="field">
        <span class="name">Page name</span>
        <input bind:value={title} oninput={() => rename(title)} maxlength="40" data-testid="page-title" />
        <span class="hint">Shown at the top left of the deck. Up to 24 characters.</span>
      </label>
      {#if titleProblem}<p class="problem" data-testid="page-title-error">{titleProblem}</p>{/if}
      {#if page.kind === 'grid'}
        <div class="field">
          <span class="name">Grid</span>
          <div class="grid-size">
            <select value={page.rows} onchange={(e) => resize(Number(e.currentTarget.value), page.cols)} aria-label="Rows" data-testid="page-rows">
              {#each SIZES as n (n)}<option value={n}>{n} {n === 1 ? 'row' : 'rows'}</option>{/each}
            </select>
            <span aria-hidden="true">×</span>
            <select value={page.cols} onchange={(e) => resize(page.rows, Number(e.currentTarget.value))} aria-label="Columns" data-testid="page-cols">
              {#each SIZES as n (n)}<option value={n}>{n} {n === 1 ? 'column' : 'columns'}</option>{/each}
            </select>
          </div>
          <span class="hint">More rows and columns make smaller buttons. Empty squares are left blank on the deck.</span>
        </div>
      {:else}
        <p class="hint">This page lists each AI account with how much of its limit is used. Set it up under AI usage. The rows on the deck are examples until then.</p>
      {/if}
      <button
        class="danger"
        class:armed={arming === page.id}
        onclick={remove}
        disabled={layout.pages.length <= 1}
        title={layout.pages.length <= 1 ? 'A layout needs at least one page' : undefined}
        data-testid="remove-page">{arming === page.id ? `Click again to remove “${page.title}”` : 'Remove this page'}</button
      >
    </section>

    <section>
      <header>
        <span class="kicker">Whole layout</span>
        <h2>All pages</h2>
      </header>
      <label class="field">
        <span class="name">Layout name</span>
        <input bind:value={name} oninput={() => renameLayout(name)} maxlength="40" data-testid="layout-name-input" />
      </label>
      {#if nameProblem}<p class="problem">{nameProblem}</p>{/if}
      <div class="toggle-row">
        <div>
          <span class="name">Use the whole screen</span>
          <p class="hint">Hide the page name and the page dots so the buttons fill the screen. You can still swipe between pages. Needs the latest deck software.</p>
        </div>
        <label class="switch">
          <input
            type="checkbox"
            checked={layout.fullScreen === true}
            onchange={(e) => apply((l) => setFullScreen(l, e.currentTarget.checked))}
            aria-label="Use the whole screen"
            data-testid="full-screen"
          />
          <span class="track"></span>
        </label>
      </div>
      {#if unseen.length}
        <p class="hint warn" data-testid="glyph-warning">The deck can't show {unseen.join(' ')}. It draws an empty box instead.</p>
      {/if}
    </section>
  </div>
{/if}

<style>
  .panel {
    display: grid;
    gap: var(--space-5);
  }

  section {
    display: grid;
    gap: var(--space-4);
  }

  section + section {
    padding-top: var(--space-5);
    border-top: 1px solid var(--line);
  }

  header {
    display: grid;
    gap: 2px;
  }

  .kicker {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .grid-size {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    gap: var(--space-2);
    color: var(--muted);
  }

  .toggle-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .toggle-row .name {
    font-size: var(--text-s);
    font-weight: 500;
  }

  .toggle-row .hint {
    margin-top: 2px;
  }

  .danger {
    justify-self: start;
  }

  .danger.armed {
    border-color: rgb(242 122 122 / 0.6);
    background: var(--bad-soft);
  }

  .warn {
    color: var(--warn);
  }
</style>
