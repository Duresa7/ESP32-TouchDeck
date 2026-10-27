<script lang="ts">
  import { missingGlyphs } from '../deck/scene.ts';
  import ActionForm from './ActionForm.svelte';
  import IconPicker from './IconPicker.svelte';
  import { friendlyRefusal } from './messages.ts';
  import { ICONS, type Action, type Edit, type Layout, type Tile, moveTile, resizeTile, setAction, setIcon, setLabel } from './validate.ts';

  interface Props {
    page: string;
    tile: Tile;
    pages: { id: string; title: string }[];
    /** Applies an edit to the layout and returns the refusal, or null when it took. */
    apply: (edit: (layout: Layout) => Edit) => string | null;
    onremove: () => void;
  }

  let { page, tile, pages, apply, onremove }: Props = $props();

  let label = $state('');
  let labelProblem = $state<string | null>(null);

  $effect.pre(() => {
    label = tile.label;
    labelProblem = null;
  });

  const LABEL_MAX = 16;
  const length = $derived([...label].length);
  const unseen = $derived(missingGlyphs(label));

  /** The deck's built-in symbols, named for what they show. */
  const SYMBOL_NAMES: Record<string, string> = {
    'lv:prev': 'Previous',
    'lv:play_pause': 'Play and pause',
    'lv:next': 'Next',
    'lv:mute': 'Speaker off',
    'lv:volume_mid': 'Speaker, quiet',
    'lv:volume_max': 'Speaker, loud',
  };

  function rename() {
    const reason = apply((l) => setLabel(l, page, tile.id, label));
    labelProblem = reason ? friendlyRefusal(reason) : null;
  }

  function number(event: Event & { currentTarget: HTMLInputElement }) {
    return Math.trunc(event.currentTarget.valueAsNumber);
  }
</script>

<div class="props" data-testid="tile-props" data-id={tile.id}>
  <header>
    <div>
      <span class="kicker">Button</span>
      <h2 title={tile.label}>{tile.label}</h2>
    </div>
    <button class="small danger" onclick={onremove} data-testid="remove-tile">Remove</button>
  </header>

  <label class="field">
    <span class="name">Name on the button</span>
    <input bind:value={label} oninput={rename} maxlength="32" data-testid="tile-label" />
    <span class="hint" class:over={length > LABEL_MAX}>{length} of {LABEL_MAX} characters. Long names get smaller on the deck.</span>
  </label>
  {#if labelProblem}
    <p class="problem" data-testid="label-error">{labelProblem}</p>
  {/if}
  {#if unseen.length}
    <p class="hint warn" data-testid="glyph-warning">The deck can't show {unseen.join(' ')}. It draws an empty box instead.</p>
  {/if}

  <section>
    <h3>When pressed</h3>
    {#key tile.id}
      <ActionForm action={tile.action} {pages} onchange={(action: Action) => apply((l) => setAction(l, page, tile.id, action))} />
    {/key}
  </section>

  <section>
    <h3>Picture</h3>
    <label class="field">
      <span class="name">Symbol</span>
      <select
        value={tile.icon ?? ''}
        onchange={(event) => apply((l) => setIcon(l, page, tile.id, event.currentTarget.value || null))}
        data-testid="tile-icon"
      >
        <option value="">None, show the name only</option>
        {#each ICONS as icon (icon)}
          <option value={icon}>{SYMBOL_NAMES[icon] ?? icon.slice(3)}</option>
        {/each}
        {#if tile.icon && !(ICONS as readonly string[]).includes(tile.icon)}
          <option value={tile.icon}>Picture from the gallery</option>
        {/if}
      </select>
    </label>
    <IconPicker {page} {tile} {apply} />
  </section>

  <details class="more position">
    <summary>Position and size</summary>
    <div class="cells">
      <label class="field"><span class="name">Row</span><input type="number" min="1" max="8" value={tile.row + 1} onchange={(e) => apply((l) => moveTile(l, page, tile.id, number(e) - 1, tile.col))} data-testid="tile-row" /></label>
      <label class="field"><span class="name">Column</span><input type="number" min="1" max="8" value={tile.col + 1} onchange={(e) => apply((l) => moveTile(l, page, tile.id, tile.row, number(e) - 1))} data-testid="tile-col" /></label>
      <label class="field"><span class="name">Rows tall</span><input type="number" min="1" max="8" value={tile.rowSpan} onchange={(e) => apply((l) => resizeTile(l, page, tile.id, number(e), tile.colSpan))} data-testid="tile-row-span" /></label>
      <label class="field"><span class="name">Columns wide</span><input type="number" min="1" max="8" value={tile.colSpan} onchange={(e) => apply((l) => resizeTile(l, page, tile.id, tile.rowSpan, number(e)))} data-testid="tile-col-span" /></label>
    </div>
    <p class="hint">On the deck you can also drag the button, or drag its corner dot. With the button selected, the arrow keys move it and Shift with an arrow key resizes it.</p>
  </details>
</div>

<style>
  .props {
    display: grid;
    gap: var(--space-4);
  }

  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3);
  }

  header > div {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  header h2 {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .kicker {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  section {
    display: grid;
    gap: var(--space-3);
    padding-top: var(--space-4);
    border-top: 1px solid var(--line);
  }

  h3 {
    font-size: var(--text-m);
  }

  .over {
    color: var(--bad);
  }

  .warn {
    color: var(--warn);
  }

  .position {
    padding-top: var(--space-4);
    border-top: 1px solid var(--line);
  }

  .position[open] .cells {
    margin-top: var(--space-3);
  }

  .cells {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3);
  }

  .position .hint {
    margin-top: var(--space-3);
    font-family: var(--font-ui);
    white-space: normal;
  }
</style>
